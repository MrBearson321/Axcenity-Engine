//! `axcenity-release`: checks and prepares releases, and answers questions
//! about the release history. Run it from anywhere in the repository:
//!
//! ```text
//! cargo run -p axcenity-release -- check [--tag v0.2.0-alpha.1]
//! cargo run -p axcenity-release -- prepare 0.2.0-alpha.1 [--date 2026-11-20]
//! cargo run -p axcenity-release -- query compare 0.1.0-alpha.3 unreleased --json
//! ```
//!
//! The release workflow also runs `plan`, `notes`, `history` and `manifest`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use axcenity_release::history::Page;
use axcenity_release::prepare::{self, RELEASES_DIR};
use axcenity_release::publish::{self, BuildFacts, HistoryFile, Manifest, Provenance};
use axcenity_release::{History, HitKind, QueryError, SearchQuery, Subsystem, Version};
use serde::Serialize;

const USAGE: &str = "usage: axcenity-release <command> [--root DIR]

  check [--tag vX.Y.Z]            check releases/, Cargo.toml and the tag agree
  prepare VERSION [--date DAY]    start the release record from releases/unreleased/
  query feature ID                a feature and its status
  query introduced ID             when a feature arrived, and how sure that is
  query timeline ID               every recorded change to a feature
  query compare FROM TO           what changed after FROM up to TO (a version or `unreleased`)
  query issue ID                  a known issue
  query issue-status ID VERSION   whether VERSION has the issue
  query issues-affecting VERSION  known issues affecting VERSION
  query minimum-engine FORMAT N   the first release that reads FORMAT version N
  query search [WORDS…] [--subsystem S] [--kind feature|issue|change|release] [--cursor C] [--limit N]

  plan --tag vX.Y.Z               what the release workflow builds for a tag (JSON)
  notes VERSION [--install FILE] [--out FILE]   release notes in Markdown
  history [--out FILE]            the public history file (JSON)
  manifest VERSION --dist DIR [--out FILE] [--commit SHA] [--published TIME]
           [--workflow-run URL] [--rustc V] [--profile P] [--source-date-epoch N]
           [--runner-image PLATFORM=IMAGE]…   the release manifest the updater trusts
  --version [--json]              this program's build

Add --json to a query for JSON.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(text) = axcenity_release::version_flag("axcenity-release", &args) {
        println!("{text}");
        return ExitCode::SUCCESS;
    }
    match run(args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

struct Args {
    positional: Vec<String>,
    root: Option<PathBuf>,
    tag: Option<String>,
    date: Option<String>,
    subsystem: Option<String>,
    kinds: Vec<String>,
    cursor: Option<String>,
    limit: usize,
    json: bool,
    /// The release workflow's options, by name without `--`.
    options: BTreeMap<&'static str, Vec<String>>,
}

impl Args {
    fn option(&self, name: &str) -> Option<&str> {
        self.options
            .get(name)
            .and_then(|v| v.last())
            .map(String::as_str)
    }
}

/// Options that take a value, for `plan`, `notes`, `history` and `manifest`.
const VALUE_OPTIONS: [&str; 10] = [
    "out",
    "install",
    "dist",
    "commit",
    "published",
    "workflow-run",
    "rustc",
    "profile",
    "source-date-epoch",
    "runner-image",
];

fn parse(args: Vec<String>) -> Result<Args, String> {
    let mut out = Args {
        positional: Vec::new(),
        root: None,
        tag: None,
        date: None,
        subsystem: None,
        kinds: Vec::new(),
        cursor: None,
        limit: 20,
        json: false,
        options: BTreeMap::new(),
    };
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        let mut value = |name: &str| {
            iter.next()
                .ok_or_else(|| format!("{name} needs a value\n\n{USAGE}"))
        };
        match arg.as_str() {
            "--root" => out.root = Some(PathBuf::from(value("--root")?)),
            "--tag" => out.tag = Some(value("--tag")?),
            "--date" => out.date = Some(value("--date")?),
            "--subsystem" => out.subsystem = Some(value("--subsystem")?),
            "--kind" => out.kinds.push(value("--kind")?),
            "--cursor" => out.cursor = Some(value("--cursor")?),
            "--limit" => {
                out.limit = value("--limit")?
                    .parse()
                    .map_err(|_| "--limit needs a number".to_owned())?
            }
            "--json" => out.json = true,
            flag if value_option(flag).is_some() => {
                let name = value_option(flag).expect("checked");
                let v = value(flag)?;
                out.options.entry(name).or_default().push(v);
            }
            "-h" | "--help" => return Err(USAGE.to_owned()),
            flag if flag.starts_with("--") => {
                return Err(format!("unknown option {flag}\n\n{USAGE}"));
            }
            _ => out.positional.push(arg),
        }
    }
    Ok(out)
}

/// `--out` → `out`, for the options in [`VALUE_OPTIONS`].
fn value_option(flag: &str) -> Option<&'static str> {
    let name = flag.strip_prefix("--")?;
    VALUE_OPTIONS.into_iter().find(|o| *o == name)
}

fn run(args: Vec<String>) -> Result<ExitCode, String> {
    let args = parse(args)?;
    let root = match &args.root {
        Some(root) => root.clone(),
        None => find_root().ok_or("run this inside the Axcenity repository, or pass --root")?,
    };
    let words: Vec<&str> = args.positional.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["check"] => {
            let problems = prepare::check(&root, args.tag.as_deref()).map_err(|e| e.to_string())?;
            if problems.is_empty() {
                println!("releases/ and Cargo.toml are consistent");
                Ok(ExitCode::SUCCESS)
            } else {
                for p in &problems {
                    eprintln!("{}: {}", p.file, p.message);
                }
                eprintln!("{} problem(s)", problems.len());
                Ok(ExitCode::FAILURE)
            }
        }
        ["prepare", version] => {
            let version = Version::parse(version).map_err(|e| e.to_string())?;
            let date = args.date.clone().unwrap_or_else(prepare::today);
            let path = prepare::prepare(&root, &version, &date).map_err(|e| e.to_string())?;
            println!(
                "Wrote {}. Write its title and summary, then run `axcenity-release check`.",
                path.display()
            );
            Ok(ExitCode::SUCCESS)
        }
        ["plan"] => {
            let tag = args.tag.as_deref().ok_or("plan needs --tag")?;
            let version = Version::from_tag(tag).map_err(|e| e.to_string())?;
            let plan = prepare::ReleasePlan::for_version(&version);
            println!(
                "{}",
                serde_json::to_string(&plan).expect("a plan serializes")
            );
            Ok(ExitCode::SUCCESS)
        }
        ["notes", version] => {
            let version = Version::parse(version).map_err(|e| e.to_string())?;
            let history = load(&root)?;
            let install = match args.option("install") {
                Some(path) => std::fs::read_to_string(path)
                    .map_err(|e| format!("could not read `{path}`: {e}"))?,
                None => String::new(),
            };
            let text =
                publish::release_notes(&history, &version, &install).map_err(|e| e.to_string())?;
            emit(&args, &text)
        }
        ["history"] => {
            let history = load(&root)?;
            let file = HistoryFile::from_history(&history);
            emit(&args, &pretty(&file))
        }
        ["manifest", version] => {
            let version = Version::parse(version).map_err(|e| e.to_string())?;
            let history = load(&root)?;
            let dist = args.option("dist").ok_or("manifest needs --dist DIR")?;
            let mut runner_images = BTreeMap::new();
            for pair in args.options.get("runner-image").into_iter().flatten() {
                let (platform, image) = pair
                    .split_once('=')
                    .ok_or("--runner-image is PLATFORM=IMAGE")?;
                runner_images.insert(platform.to_owned(), image.to_owned());
            }
            let source_date_epoch = args
                .option("source-date-epoch")
                .map(|n| {
                    n.parse::<u64>()
                        .map_err(|_| "--source-date-epoch needs a number")
                })
                .transpose()?;
            let text_of = |name: &str| args.option(name).unwrap_or_default().to_owned();
            let facts = BuildFacts {
                commit: text_of("commit"),
                published: text_of("published"),
                provenance: Provenance {
                    workflow_run: text_of("workflow-run"),
                    rustc: text_of("rustc"),
                    profile: text_of("profile"),
                    runner_images,
                    source_date_epoch,
                    reproducible: String::new(),
                },
            };
            let manifest = Manifest::build(&history, &version, Path::new(dist), facts)
                .map_err(|e| e.to_string())?;
            emit(&args, &pretty(&manifest))
        }
        ["query", rest @ ..] => {
            let history = History::load(&root.join(RELEASES_DIR)).map_err(|e| e.to_string())?;
            query(&history, rest, &args)
        }
        _ => Err(USAGE.to_owned()),
    }
}

fn query(history: &History, words: &[&str], args: &Args) -> Result<ExitCode, String> {
    let answer: Result<serde_json::Value, QueryError> = match words {
        ["feature", id] => history.feature(id).map(to_json),
        ["introduced", id] => history.introduced_in(id).map(to_json),
        ["timeline", id] => history.feature_timeline(id).map(|t| to_json(&t)),
        ["compare", from, to] => history
            .version(from, false)
            .and_then(|from| Ok((from, history.version(to, true)?)))
            .and_then(|(from, to)| history.compare(&from, &to))
            .map(|c| to_json(&c)),
        ["issue", id] => history.issue(id).map(to_json),
        ["issue-status", id, version] => history.version(version, false).and_then(|v| {
            history.issue_status(id, v.release().expect("a release"))
                .map(|s| to_json(&s))
        }),
        ["issues-affecting", version] => history.version(version, false).map(|v| {
            let list: Vec<_> = history
                .issues_affecting(v.release().expect("a release"))
                .into_iter()
                .map(|(issue, state)| serde_json::json!({"id": issue.id, "title": issue.title, "state": state}))
                .collect();
            serde_json::Value::Array(list)
        }),
        ["minimum-engine", format, n] => {
            let n: u32 = n.parse().map_err(|_| "the format version is a number")?;
            Ok(serde_json::json!(history.minimum_engine_for(format, n).map(|v| v.to_string())))
        }
        ["search", text @ ..] => {
            let subsystem = args
                .subsystem
                .as_deref()
                .map(|s| s.parse::<Subsystem>())
                .transpose()?;
            let kinds = args
                .kinds
                .iter()
                .map(|k| match k.as_str() {
                    "feature" => Ok(HitKind::Feature),
                    "issue" => Ok(HitKind::Issue),
                    "change" => Ok(HitKind::Change),
                    "release" => Ok(HitKind::Release),
                    _ => Err(format!("`{k}` is not a kind: feature, issue, change or release")),
                })
                .collect::<Result<_, _>>()?;
            let search = SearchQuery {
                text: text.join(" "),
                subsystem,
                kinds,
            };
            history
                .search(&search, args.cursor.as_deref(), args.limit)
                .map(|page: Page<_>| to_json(&page))
        }
        _ => return Err(USAGE.to_owned()),
    };
    match answer {
        Ok(value) => {
            if args.json {
                println!("{}", serde_json::to_string_pretty(&value).unwrap());
            } else {
                println!("{}", plain(&value, 0));
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => {
            if args.json {
                println!("{}", serde_json::to_string_pretty(&err).unwrap());
            } else {
                eprintln!("{err}");
            }
            Ok(ExitCode::FAILURE)
        }
    }
}

fn load(root: &Path) -> Result<History, String> {
    History::load(&root.join(RELEASES_DIR)).map_err(|e| e.to_string())
}

fn pretty<T: Serialize>(value: &T) -> String {
    let mut text = serde_json::to_string_pretty(value).expect("published files serialize");
    text.push('\n');
    text
}

/// Writes `text` to `--out`, or prints it.
fn emit(args: &Args, text: &str) -> Result<ExitCode, String> {
    match args.option("out") {
        Some(path) => {
            std::fs::write(path, text).map_err(|e| format!("could not write `{path}`: {e}"))?;
            eprintln!("wrote {path}");
        }
        None => print!("{text}"),
    }
    Ok(ExitCode::SUCCESS)
}

fn to_json<T: Serialize + ?Sized>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).expect("query answers serialize")
}

/// A readable indented form of a JSON answer, for people at a terminal.
fn plain(value: &serde_json::Value, depth: usize) -> String {
    let pad = "  ".repeat(depth);
    match value {
        serde_json::Value::Object(map) => map
            .iter()
            .filter(|(_, v)| !v.is_null())
            .map(|(k, v)| match v {
                serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
                    format!("{pad}{k}:\n{}", plain(v, depth + 1))
                }
                _ => format!("{pad}{k}: {}", plain(v, 0)),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        serde_json::Value::Array(items) if items.is_empty() => format!("{pad}(none)"),
        serde_json::Value::Array(items) => items
            .iter()
            .map(|v| match v {
                serde_json::Value::Object(_) => format!("{pad}-\n{}", plain(v, depth + 1)),
                _ => format!("{pad}- {}", plain(v, 0)),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The repository root: the nearest folder up from here with `releases/`
/// and a `Cargo.toml`.
fn find_root() -> Option<PathBuf> {
    let mut dir: &Path = &std::env::current_dir().ok()?;
    loop {
        if dir.join(RELEASES_DIR).is_dir() && dir.join("Cargo.toml").is_file() {
            return Some(dir.to_owned());
        }
        dir = dir.parent()?;
    }
}
