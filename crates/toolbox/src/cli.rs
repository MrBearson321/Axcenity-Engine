//! `axcenity-toolbox`, the command line the Blender integration and
//! scripts use: the same commands as the Blender pipeline's
//! `tools/library.py`, over the same sources, asset directory and metadata
//! database as the editor.
//!
//! ```text
//! axcenity-toolbox search "red brick" --kind texture [--provider polyhaven] [--limit 15] [--json]
//! axcenity-toolbox info polyhaven:red_brick_03 [--json]
//! axcenity-toolbox fetch polyhaven:red_brick_03 [--res 2k] [--format jpg] [--json]
//! axcenity-toolbox list [--kind texture] [--query brick] [--downloaded] [--json]
//! axcenity-toolbox credits [--keys a:b,c:d | --project <folder>] [--out CREDITS.txt]
//! axcenity-toolbox stats [--json]
//! axcenity-toolbox where
//! axcenity-toolbox migrate <old asset_library> [--dry-run] [--json]
//!   common: --dir <asset directory>  --offline
//! ```
//!
//! Exit codes: 0 done, 1 a usage mistake, 2 a [`SourceError`] (its safe
//! message on stderr), 3 a source unavailable (offline, or every source
//! failed). `--json` prints records in the `asset-source` shape and
//! downloads in the database's.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde_json::{Value, json};

use crate::db::{Database, Download};
use crate::directory::{self, AssetDirectory};
use crate::download::{Fetcher, extract_for};
use crate::http::Http;
use crate::library::Filter;
use crate::provenance::{self, Snapshot};
use crate::record::{AssetRecord, Kind};
use crate::{AssetSource, Query, SourceError, migrate, source, sources_for, variants};

/// What a run needs from outside: the HTTP client, where the asset
/// directory is when `--dir` isn't given, and where output goes.
pub struct Env<'a> {
    pub http: &'a dyn Http,
    pub default_dir: Option<PathBuf>,
    pub out: &'a mut dyn Write,
    pub err: &'a mut dyn Write,
}

const USAGE: &str =
    "usage: axcenity-toolbox <search|info|fetch|list|credits|stats|where|migrate> [options]
  search <words> [--kind K] [--provider P] [--limit N] [--json]
  info <provider:id> [--json]
  fetch <provider:id> [--res 2k] [--format jpg] [--json]
  list [--kind K] [--query words] [--downloaded] [--json]
  credits [--keys a:b,c:d | --project <folder>] [--out FILE]
  stats [--json]
  where
  migrate <old asset_library> [--dry-run] [--json]
common: --dir <asset directory>  --offline";

/// Parsed options: the words, and `--name value` or `--flag`.
struct Args {
    words: Vec<String>,
    options: Vec<(String, Option<String>)>,
}

const FLAGS: [&str; 4] = ["--json", "--offline", "--downloaded", "--dry-run"];

impl Args {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut words = Vec::new();
        let mut options = Vec::new();
        let mut rest = args.iter();
        while let Some(arg) = rest.next() {
            if let Some(name) = arg.strip_prefix("--") {
                let (name, inline) = match name.split_once('=') {
                    Some((name, value)) => (name, Some(value.to_owned())),
                    None => (name, None),
                };
                let name = format!("--{name}");
                if FLAGS.contains(&name.as_str()) {
                    options.push((name, None));
                } else {
                    let value = match inline {
                        Some(value) => value,
                        None => rest
                            .next()
                            .cloned()
                            .ok_or_else(|| format!("{name} needs a value"))?,
                    };
                    options.push((name, Some(value)));
                }
            } else {
                words.push(arg.clone());
            }
        }
        Ok(Self { words, options })
    }

    fn flag(&self, name: &str) -> bool {
        self.options.iter().any(|(option, _)| option == name)
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.options
            .iter()
            .rev()
            .find(|(option, _)| option == name)
            .and_then(|(_, value)| value.as_deref())
    }
}

enum Failure {
    Usage(String),
    Source(SourceError),
    Unavailable(String),
}

impl From<SourceError> for Failure {
    fn from(err: SourceError) -> Self {
        match err {
            SourceError::Network(_) => Failure::Unavailable(err.to_string()),
            other => Failure::Source(other),
        }
    }
}

impl From<std::io::Error> for Failure {
    fn from(err: std::io::Error) -> Self {
        Failure::Source(err.into())
    }
}

/// Runs one command; returns the exit code.
pub fn run(args: &[String], env: &mut Env) -> i32 {
    let help = |arg: &String| matches!(arg.as_str(), "--help" | "-h" | "help");
    if args.first().is_some_and(help) || args.iter().any(|arg| arg == "--help") {
        let _ = writeln!(env.out, "{USAGE}");
        return 0;
    }
    match command(args, env) {
        Ok(()) => 0,
        Err(Failure::Usage(text)) => {
            let _ = writeln!(env.err, "{text}\n{USAGE}");
            1
        }
        Err(Failure::Source(err)) => {
            let _ = writeln!(env.err, "axcenity-toolbox: {err}");
            2
        }
        Err(Failure::Unavailable(text)) => {
            let _ = writeln!(env.err, "axcenity-toolbox: {text}");
            3
        }
    }
}

fn command(args: &[String], env: &mut Env) -> Result<(), Failure> {
    let args = Args::parse(args).map_err(Failure::Usage)?;
    let Some((name, words)) = args.words.split_first() else {
        return Err(Failure::Usage("no command".into()));
    };
    let words = words.to_vec();
    let root = match args.value("--dir") {
        Some(dir) => Some(PathBuf::from(dir)),
        None => env.default_dir.clone(),
    };
    let open = || -> Result<(AssetDirectory, Database), Failure> {
        let root = root.clone().ok_or_else(|| {
            Failure::Usage(format!(
                "there is no asset directory; pass --dir or set {}",
                directory::ENV
            ))
        })?;
        let dir = AssetDirectory::open(&root)?;
        let db = Database::open(&dir.database_path())?;
        Ok((dir, db))
    };
    let offline = args.flag("--offline");
    let as_json = args.flag("--json");
    match name.as_str() {
        "where" => {
            let root = root.ok_or_else(|| Failure::Usage("there is no asset directory".into()))?;
            writeln!(env.out, "{}", root.display())?;
        }
        "search" => {
            let kind = kind(&args)?;
            let text = words.join(" ");
            let limit = number(&args, "--limit", 15)?;
            let (_, db) = open()?;
            let records = if offline {
                let mut found = db.assets(&Filter {
                    kind: Some(kind),
                    text,
                    downloaded: false,
                })?;
                if let Some(provider) = args.value("--provider") {
                    found.retain(|record| record.provider == provider);
                }
                found
            } else {
                search(env, &db, kind, &text, args.value("--provider"), limit)?
            };
            let records: Vec<AssetRecord> = records.into_iter().take(limit).collect();
            if as_json {
                print_json(env.out, &json!(records))?;
            } else {
                for record in &records {
                    writeln!(
                        env.out,
                        "{}\t{}\t{}",
                        record.key(),
                        record.license,
                        record.name
                    )?;
                }
            }
        }
        "info" => {
            let key = one_key(&words)?;
            let (_, db) = open()?;
            let record = lookup(env.http, &db, &key, offline)?;
            if as_json {
                print_json(env.out, &json!(record))?;
            } else {
                writeln!(env.out, "{}\n{}", record.name, record.key())?;
                writeln!(
                    env.out,
                    "licence: {} {}",
                    record.license, record.license_version
                )?;
                for variant in &record.variants {
                    writeln!(
                        env.out,
                        "  {} ({} files)",
                        variant.label,
                        variant.files.len()
                    )?;
                }
            }
        }
        "fetch" => {
            let key = one_key(&words)?;
            let (dir, db) = open()?;
            let record = lookup(env.http, &db, &key, offline)?;
            let resolution = args.value("--res").unwrap_or("2k").to_ascii_lowercase();
            let format = args.value("--format").unwrap_or("").to_ascii_lowercase();
            let choice = variants::choose(&record.variants, &resolution, &format, &[]).ok_or(
                SourceError::BadResponse("the asset has nothing to download"),
            )?;
            let (variant, notes) = (choice.variant.clone(), choice.notes);
            let download = match db.download(&key, &variant.label)? {
                Some(download) if crate::download::is_present(&dir, &download) => download,
                _ if offline => {
                    return Err(Failure::Unavailable(
                        "that download isn't in the library and --offline was given".into(),
                    ));
                }
                _ => {
                    let source = source(&record.provider).ok_or(SourceError::Refused(
                        "that provider can't be fetched from here",
                    ))?;
                    let fetcher = Fetcher {
                        http: env.http,
                        dir: &dir,
                        db: &db,
                    };
                    fetcher.fetch_variant(
                        source.info(),
                        &record,
                        &variant,
                        extract_for(record.kind),
                        &AtomicBool::new(false),
                    )?
                }
            };
            db.touch_recent(&key)?;
            if as_json {
                let mut value = download_json(&dir, &download);
                value["notes"] = json!(notes);
                print_json(env.out, &value)?;
            } else {
                writeln!(env.out, "{}", dir.path(&download.folder).display())?;
                for note in notes {
                    writeln!(env.err, "{note}")?;
                }
            }
        }
        "list" => {
            let (dir, db) = open()?;
            let filter = Filter {
                kind: match args.value("--kind") {
                    Some(_) => Some(kind(&args)?),
                    None => None,
                },
                text: args.value("--query").unwrap_or("").to_owned(),
                downloaded: args.flag("--downloaded"),
            };
            let records = db.assets(&filter)?;
            if as_json {
                let mut rows = Vec::new();
                for record in &records {
                    let downloads: Vec<Value> = db
                        .downloads_of(&record.key())?
                        .iter()
                        .map(|download| download_json(&dir, download))
                        .collect();
                    rows.push(json!({ "record": record, "downloads": downloads }));
                }
                print_json(env.out, &json!(rows))?;
            } else {
                for record in &records {
                    writeln!(env.out, "{}\t{}", record.key(), record.name)?;
                }
            }
        }
        "credits" => {
            let (_, db) = open()?;
            let (title, records) = if let Some(project) = args.value("--project") {
                let project = Path::new(project);
                let snapshot = Snapshot::read(project)?;
                let title = project.file_name().map_or("This game".into(), |name| {
                    name.to_string_lossy().into_owned()
                });
                let records: Vec<AssetRecord> = snapshot.records().into_iter().cloned().collect();
                (title, records)
            } else if let Some(keys) = args.value("--keys") {
                let keys: Vec<String> = keys
                    .split(',')
                    .map(|key| key.trim().to_owned())
                    .filter(|key| !key.is_empty())
                    .collect();
                ("These assets".to_owned(), db.assets_by_key(&keys)?)
            } else {
                let downloaded = db.assets(&Filter {
                    downloaded: true,
                    ..Filter::default()
                })?;
                ("The asset library".to_owned(), downloaded)
            };
            let borrowed: Vec<&AssetRecord> = records.iter().collect();
            let text = provenance::credits(&title, &borrowed)
                .unwrap_or_else(|| "No third-party assets.\n".to_owned());
            match args.value("--out") {
                Some(path) => std::fs::write(path, text)?,
                None => write!(env.out, "{text}")?,
            }
        }
        "stats" => {
            let (dir, db) = open()?;
            let stats = db.stats()?;
            if as_json {
                let mut value = json!(stats);
                value["directory"] = json!(dir.root().display().to_string());
                print_json(env.out, &value)?;
            } else {
                writeln!(
                    env.out,
                    "{} assets, {} downloads ({:.1} MB), {} installs, {} favourites, {} collections",
                    stats.assets,
                    stats.downloads,
                    stats.bytes as f64 / 1_000_000.0,
                    stats.installs,
                    stats.favourites,
                    stats.collections
                )?;
            }
        }
        "migrate" => {
            let [old] = words.as_slice() else {
                return Err(Failure::Usage(
                    "migrate needs the old asset_library folder".into(),
                ));
            };
            let (dir, db) = open()?;
            let report =
                migrate::import_blender_library(Path::new(old), &dir, &db, args.flag("--dry-run"))?;
            if as_json {
                print_json(env.out, &json!(report))?;
            } else {
                writeln!(env.out, "{}", report.summary())?;
            }
        }
        other => return Err(Failure::Usage(format!("unknown command {other:?}"))),
    }
    Ok(())
}

fn kind(args: &Args) -> Result<Kind, Failure> {
    let text = args.value("--kind").unwrap_or("texture");
    Kind::parse(text).ok_or_else(|| Failure::Usage(format!("unknown kind {text:?}")))
}

fn number(args: &Args, name: &str, default: usize) -> Result<usize, Failure> {
    match args.value(name) {
        None => Ok(default),
        Some(text) => text
            .parse()
            .map_err(|_| Failure::Usage(format!("{name} needs a number"))),
    }
}

fn one_key(words: &[String]) -> Result<String, Failure> {
    match words {
        [key] if key.contains(':') => Ok(key.clone()),
        _ => Err(Failure::Usage("give one provider:id".into())),
    }
}

/// Every source with `kind` (or just `provider`), their results taken in
/// turn (one from each source, then the next from each), so the first
/// `limit` show every source rather than only the first. A failing source
/// is reported on stderr; if every one fails, that's the error.
fn search(
    env: &mut Env,
    db: &Database,
    kind: Kind,
    text: &str,
    provider: Option<&str>,
    limit: usize,
) -> Result<Vec<AssetRecord>, Failure> {
    let chosen: Vec<&dyn AssetSource> = sources_for(kind)
        .into_iter()
        .filter(|source| provider.is_none_or(|key| source.info().key == key))
        .collect();
    if chosen.is_empty() {
        return Err(Failure::Usage("no source has that kind".into()));
    }
    let mut query = Query::new(text, kind);
    query.per_page = limit.clamp(1, 50) as u32;
    let mut pages = Vec::new();
    let mut failures = Vec::new();
    for source in &chosen {
        match source.search(env.http, &query) {
            Ok(page) => {
                for record in &page.records {
                    db.put_asset(record)?;
                }
                pages.push(page.records);
            }
            Err(err) => {
                let _ = writeln!(env.err, "{}: {err}", source.info().name);
                failures.push(err);
            }
        }
    }
    if failures.len() == chosen.len() {
        return Err(Failure::Unavailable("every source failed".into()));
    }
    Ok(in_turn(pages))
}

/// One record from each list, then the next from each, until all are used.
fn in_turn(lists: Vec<Vec<AssetRecord>>) -> Vec<AssetRecord> {
    let mut lists: Vec<_> = lists.into_iter().map(Vec::into_iter).collect();
    let mut records = Vec::new();
    loop {
        let before = records.len();
        records.extend(lists.iter_mut().filter_map(Iterator::next));
        if records.len() == before {
            return records;
        }
    }
}

/// The full record: from the source (and stored), or offline the stored one.
fn lookup(
    http: &dyn Http,
    db: &Database,
    key: &str,
    offline: bool,
) -> Result<AssetRecord, Failure> {
    let stored = db.asset(key)?;
    if offline {
        return stored.ok_or_else(|| {
            Failure::Unavailable("that asset isn't in the library and --offline was given".into())
        });
    }
    let (provider, id) = key.split_once(':').unwrap_or_default();
    let Some(source) = source(provider) else {
        return stored.ok_or(Failure::Source(SourceError::NotFound));
    };
    let record = source.record(http, id)?;
    db.put_asset(&record)?;
    Ok(record)
}

fn download_json(dir: &AssetDirectory, download: &Download) -> Value {
    json!({
        "key": download.key,
        "variant": download.variant,
        "folder": download.folder,
        "path": dir.path(&download.folder).display().to_string(),
        "files": download.files,
        "bytes": download.bytes,
        "source_urls": download.source_urls,
        "fetched": download.fetched,
    })
}

fn print_json(out: &mut dyn Write, value: &Value) -> Result<(), Failure> {
    let text = serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".into());
    writeln!(out, "{text}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Fixture;
    use crate::openverse;

    const SEARCH: &str = r#"{
        "result_count": 1, "page_count": 1,
        "results": [
            {"id": "4bd0a3e8-1111-2222-3333-444455556666", "title": "Forest Birds", "creator": "Jane Doe",
             "license": "by", "license_version": "4.0", "license_url": "https://creativecommons.org/licenses/by/4.0/",
             "attribution": "\"Forest Birds\" by Jane Doe is licensed under CC BY 4.0.",
             "foreign_landing_url": "https://freesound.org/s/1/", "url": "https://cdn.freesound.org/previews/1/birds.mp3",
             "filetype": "mp3", "filesize": 5,
             "category": "sound_effect", "tags": [{"name": "birds"}]}
        ]
    }"#;
    const ID: &str = "4bd0a3e8-1111-2222-3333-444455556666";

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("axcenity-cli-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fixture() -> Fixture {
        let mut query = Query::new("birds", Kind::Sound);
        query.per_page = 15;
        let record = SEARCH
            .split_once("\"results\": [")
            .unwrap()
            .1
            .rsplit_once(']')
            .unwrap()
            .0;
        Fixture::default()
            .with(&openverse::search_url(&query).unwrap(), SEARCH)
            .with(&openverse::record_url(ID), record.trim())
            .with("https://cdn.freesound.org/previews/1/birds.mp3", "chirp")
    }

    fn run_with(http: &dyn Http, dir: &Path, args: &[&str]) -> (i32, String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let mut env = Env {
            http,
            default_dir: Some(dir.to_owned()),
            out: &mut out,
            err: &mut err,
        };
        let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
        let code = run(&args, &mut env);
        (
            code,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    #[test]
    fn search_info_fetch_list_and_credits_work_together() {
        let dir = temp("flow");
        let http = fixture();
        let (code, out, err) = run_with(
            &http,
            &dir,
            &["search", "birds", "--kind", "sound", "--json"],
        );
        assert_eq!(code, 0, "{err}");
        let found: Vec<AssetRecord> = serde_json::from_str(&out).unwrap();
        assert_eq!(found[0].key(), format!("openverse:{ID}"));

        let key = format!("openverse:{ID}");
        let (code, out, err) = run_with(&http, &dir, &["fetch", &key, "--json"]);
        assert_eq!(code, 0, "{err}");
        let fetched: Value = serde_json::from_str(&out).unwrap();
        let folder = PathBuf::from(fetched["path"].as_str().unwrap());
        assert_eq!(
            std::fs::read(folder.join(fetched["files"][0]["path"].as_str().unwrap())).unwrap(),
            b"chirp"
        );

        // Offline, everything is answered from the library.
        let nothing = Fixture::default();
        let (code, out, _) = run_with(&nothing, &dir, &["fetch", &key, "--offline", "--json"]);
        assert_eq!(code, 0);
        assert!(out.contains("\"bytes\": 5"));
        let (code, out, _) = run_with(
            &nothing,
            &dir,
            &["search", "forest", "--kind", "sounds", "--offline"],
        );
        assert_eq!(code, 0);
        assert!(out.contains("Forest Birds"));
        let (code, out, _) = run_with(&nothing, &dir, &["list", "--downloaded", "--json"]);
        assert_eq!(code, 0);
        let rows: Vec<Value> = serde_json::from_str(&out).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["downloads"][0]["bytes"], 5);

        let (code, out, _) = run_with(&nothing, &dir, &["credits"]);
        assert_eq!(code, 0);
        assert!(out.contains("\"Forest Birds\" by Jane Doe is licensed under CC BY 4.0."));
        let (code, out, _) = run_with(&nothing, &dir, &["stats", "--json"]);
        assert_eq!(code, 0);
        assert!(out.contains("\"downloads\": 1"));
        let (code, out, _) = run_with(&nothing, &dir, &["where"]);
        assert_eq!((code, out.trim()), (0, dir.display().to_string().as_str()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn help_prints_the_usage_and_succeeds() {
        let dir = temp("help");
        let nothing = Fixture::default();
        for args in [&["--help"][..], &["-h"], &["help"], &["search", "--help"]] {
            let (code, out, err) = run_with(&nothing, &dir, args);
            assert_eq!((code, err.as_str()), (0, ""), "{args:?}");
            assert!(out.starts_with("usage: axcenity-toolbox"), "{args:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_search_of_every_source_takes_from_each_in_turn() {
        let record = |provider: &str, id: &str| AssetRecord {
            provider: provider.into(),
            id: id.into(),
            ..AssetRecord::default()
        };
        let lists = vec![
            vec![
                record("ambientcg", "a1"),
                record("ambientcg", "a2"),
                record("ambientcg", "a3"),
            ],
            vec![record("polyhaven", "p1")],
        ];
        let keys: Vec<String> = in_turn(lists).iter().map(AssetRecord::key).collect();
        assert_eq!(
            keys,
            [
                "ambientcg:a1",
                "polyhaven:p1",
                "ambientcg:a2",
                "ambientcg:a3"
            ]
        );
    }

    #[test]
    fn mistakes_and_failures_have_their_own_exit_codes() {
        let dir = temp("codes");
        let nothing = Fixture::default();
        assert_eq!(run_with(&nothing, &dir, &[]).0, 1);
        assert_eq!(run_with(&nothing, &dir, &["dance"]).0, 1);
        assert_eq!(run_with(&nothing, &dir, &["info", "no-colon"]).0, 1);
        assert_eq!(
            run_with(&nothing, &dir, &["search", "x", "--kind", "planets"]).0,
            1
        );
        assert_eq!(run_with(&nothing, &dir, &["search", "x", "--limit"]).0, 1);
        // The fixture has no answer: the source is "not found", a source error.
        let (code, _, err) = run_with(&nothing, &dir, &["info", "openverse:missing-id"]);
        assert_eq!(code, 2);
        assert!(err.contains("no asset with that id"));
        let (code, _, _) = run_with(&nothing, &dir, &["fetch", "openverse:missing", "--offline"]);
        assert_eq!(code, 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
