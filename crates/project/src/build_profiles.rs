//! Build profiles and build reports (roadmap R13).
//!
//! A build profile is the saved settings Build Game uses: which computer
//! the game is for, whether texture sources are left out for their cooked
//! copies, how hard the game file is compressed, the Developer Console, and
//! whether Asset Check problems stop the build. Two are built in (Release,
//! which builds as Build Game always has, and Development); a project adds
//! its own, or changes a built-in one, in `build-profiles.json`:
//!
//! ```json
//! {
//!   "format_version": 1,
//!   "profiles": {
//!     "Shipping": { "target": "windows", "compression_level": 9, "stop_on_problems": true }
//!   }
//! }
//! ```
//!
//! Each build writes a report beside the game ([`BuildReport`]): when and
//! with what it was built (engine version, the profile's settings, the
//! project's Git commit), what's in the game file by kind and its largest
//! files, the texture sources left out, Asset Check's problems and the
//! build's notes. The report's settings are a profile, so a build can be
//! made again the same way.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{PackOptions, PackReport};

/// The project file with its own build profiles.
pub const BUILD_PROFILES_FILE: &str = "build-profiles.json";

/// The profile Build Game uses when none is picked: the build it always made.
pub const RELEASE_PROFILE: &str = "Release";

/// The report each build writes beside the game, as JSON and as text.
pub const BUILD_REPORT_FILE: &str = "build-report.json";
pub const BUILD_REPORT_TEXT_FILE: &str = "build-report.txt";

const FORMAT_VERSION: u32 = 1;

/// The computers a profile can build for, as `target` names them.
pub const TARGET_NAMES: [&str; 3] = ["windows", "macos", "linux"];

/// How Build Game builds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildProfile {
    /// `windows`, `macos` or `linux`; none builds for this computer's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Leave each texture's source out when its cooked copy ships (G12).
    #[serde(default = "yes")]
    pub cooked_only: bool,
    /// zstd's level for the game file, 1 (fastest) to 19 (smallest); 0 is
    /// the usual 3.
    #[serde(default)]
    pub compression_level: i32,
    /// The Developer Console (F9) in the game; none keeps the project's
    /// setting (File > Build Game > Developer Console).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub developer_console: Option<bool>,
    /// Don't build when Asset Check finds a missing file, a bad reference,
    /// a broken dependency or a file that won't import.
    #[serde(default)]
    pub stop_on_problems: bool,
}

fn yes() -> bool {
    true
}

impl Default for BuildProfile {
    fn default() -> Self {
        BuildProfile {
            target: None,
            cooked_only: true,
            compression_level: 0,
            developer_console: None,
            stop_on_problems: false,
        }
    }
}

impl BuildProfile {
    /// How the game file is packed with this profile.
    pub fn pack_options(&self) -> PackOptions {
        PackOptions {
            cooked_only: self.cooked_only,
            compression_level: self.compression_level,
            developer_console: self.developer_console,
        }
    }

    /// A few words on what it does, for a menu's tooltip.
    pub fn describe(&self) -> String {
        let target = self
            .target
            .as_deref()
            .map_or("this computer", |target| match target {
                "windows" => "Windows",
                "macos" => "macOS",
                _ => "Linux",
            });
        let sources = if self.cooked_only {
            "cooked textures only"
        } else {
            "texture sources kept"
        };
        let console = match self.developer_console {
            Some(true) => ", Developer Console on",
            Some(false) => ", Developer Console off",
            None => "",
        };
        let level = match self.compression_level {
            0 => String::new(),
            level => format!(", compression {level}"),
        };
        let stop = if self.stop_on_problems {
            ", stops on Asset Check problems"
        } else {
            ""
        };
        format!("For {target}, {sources}{level}{console}{stop}.")
    }
}

/// The profiles every project has: Release first.
pub fn built_in_profiles() -> [(&'static str, BuildProfile); 2] {
    [
        (RELEASE_PROFILE, BuildProfile::default()),
        (
            "Development",
            BuildProfile {
                target: None,
                cooked_only: false,
                compression_level: 1,
                developer_console: Some(true),
                stop_on_problems: false,
            },
        ),
    ]
}

/// A project's `build-profiles.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildProfiles {
    #[serde(default = "format_version")]
    pub format_version: u32,
    /// The project's own profiles by name; one named like a built-in one
    /// takes its place.
    #[serde(default)]
    pub profiles: BTreeMap<String, BuildProfile>,
}

fn format_version() -> u32 {
    FORMAT_VERSION
}

impl BuildProfiles {
    /// The project's profiles file, or none (only the built-in profiles)
    /// if it has none. An error says why a file that's there can't be used.
    pub fn read(project: &Path) -> Result<BuildProfiles, String> {
        let bytes = match fs::read(project.join(BUILD_PROFILES_FILE)) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(BuildProfiles::default());
            }
            Err(err) => return Err(err.to_string()),
        };
        let profiles: BuildProfiles =
            serde_json::from_slice(&bytes).map_err(|err| err.to_string())?;
        if profiles.format_version > FORMAT_VERSION {
            return Err(format!(
                "it's format {}, newer than this editor reads",
                profiles.format_version
            ));
        }
        for (name, profile) in &profiles.profiles {
            if let Some(target) = &profile.target
                && !TARGET_NAMES.contains(&target.as_str())
            {
                return Err(format!(
                    "the profile `{name}` builds for `{target}`; use windows, macos or linux"
                ));
            }
        }
        Ok(profiles)
    }

    /// Every profile's name: the built-in ones, then the project's own.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = built_in_profiles()
            .iter()
            .map(|(name, _)| (*name).to_owned())
            .collect();
        for name in self.profiles.keys() {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        names
    }

    /// The profile called `name`, if there is one.
    pub fn get(&self, name: &str) -> Option<BuildProfile> {
        self.profiles.get(name).cloned().or_else(|| {
            built_in_profiles()
                .into_iter()
                .find(|(built_in, _)| *built_in == name)
                .map(|(_, profile)| profile)
        })
    }
}

/// A group of the game's files in a report, such as "Cooked textures".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportGroup {
    pub name: String,
    pub files: usize,
    /// Their size unpacked.
    pub bytes: u64,
    /// What they take in the game file.
    pub stored: u64,
}

/// One file in a report's list of the largest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportFile {
    pub path: String,
    pub bytes: u64,
    pub stored: u64,
}

/// What a build made and how (R13), written beside the game.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BuildReport {
    pub format_version: u32,
    /// When, in UTC (`2026-11-12T23:10:05Z`).
    pub built_at: String,
    /// The editor that built it.
    pub engine_version: String,
    /// The game's name (the manifest's).
    pub project: String,
    /// The project folder's Git commit, when it's a Git repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_commit: Option<String>,
    pub profile: String,
    /// The profile's settings as built, so the build can be made again.
    pub settings: BuildProfile,
    /// `windows`, `macos` or `linux`.
    pub target: String,
    /// How long it took, in seconds.
    pub seconds: f64,
    /// The game file's size, and a hash of its bytes (two builds of the
    /// same files with the same settings have the same one).
    pub game_file_bytes: u64,
    pub game_file_hash: String,
    pub files: usize,
    pub groups: Vec<ReportGroup>,
    /// The largest files, by the bytes they take in the game file.
    pub largest: Vec<ReportFile>,
    /// Texture sources left out for their cooked copies: how many, and
    /// the bytes they'd have been.
    pub left_out_files: usize,
    pub left_out_bytes: u64,
    /// Asset Check's problems before packing, one line each.
    pub problems: Vec<String>,
    /// Textures that ship as they are, and why.
    pub uncooked: Vec<String>,
    /// Anything else to know (a bad icon, an unsigned app).
    pub notes: Vec<String>,
}

/// How many of the largest files a report lists.
pub const LARGEST_LISTED: usize = 20;

/// Which group of a report the game file `name` belongs to.
pub fn report_group(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    let extension = lower
        .rsplit_once('.')
        .map_or("", |(_, extension)| extension);
    if lower.starts_with(".axcenity/imported/textures/") {
        "Cooked textures"
    } else if lower.starts_with(".axcenity/imported/") {
        "Cooked models and meshes"
    } else if extension == "lua" || lower.starts_with("scripts/") || lower.starts_with("objects/") {
        "Scripts"
    } else if axcenity_assets::TEXTURE_EXTENSIONS.contains(&extension) {
        "Textures"
    } else if ["glb", "gltf", "fbx", "obj", "mtl", "bin"].contains(&extension) {
        "Models"
    } else if axcenity_assets::AUDIO_EXTENSIONS.contains(&extension) {
        "Audio"
    } else if ["ttf", "otf"].contains(&extension) {
        "Fonts"
    } else if lower == "world.json" || lower.starts_with("levels/") {
        "World and levels"
    } else {
        "Other"
    }
}

impl BuildReport {
    /// A report of `pack` (the game file `game`), built with `profile`
    /// (`settings`) for `target` in `seconds`. The lists start empty.
    pub fn new(
        folder: &Path,
        profile: &str,
        settings: &BuildProfile,
        target: &str,
        game: &[u8],
        pack: &PackReport,
    ) -> BuildReport {
        let mut groups: BTreeMap<&str, ReportGroup> = BTreeMap::new();
        for entry in &pack.entries {
            let name = report_group(&entry.name);
            let group = groups.entry(name).or_insert_with(|| ReportGroup {
                name: name.to_owned(),
                files: 0,
                bytes: 0,
                stored: 0,
            });
            group.files += 1;
            group.bytes += entry.bytes;
            group.stored += entry.stored;
        }
        let mut groups: Vec<ReportGroup> = groups.into_values().collect();
        groups.sort_by(|a, b| b.stored.cmp(&a.stored).then_with(|| a.name.cmp(&b.name)));
        let mut largest: Vec<ReportFile> = pack
            .entries
            .iter()
            .map(|entry| ReportFile {
                path: entry.name.clone(),
                bytes: entry.bytes,
                stored: entry.stored,
            })
            .collect();
        largest.sort_by(|a, b| b.stored.cmp(&a.stored).then_with(|| a.path.cmp(&b.path)));
        largest.truncate(LARGEST_LISTED);
        BuildReport {
            format_version: FORMAT_VERSION,
            built_at: utc_now(),
            engine_version: String::new(),
            project: crate::read_manifest(folder)
                .map(|manifest| manifest.name)
                .unwrap_or_default(),
            project_commit: git_commit(folder),
            profile: profile.to_owned(),
            settings: settings.clone(),
            target: target.to_owned(),
            seconds: 0.0,
            game_file_bytes: game.len() as u64,
            game_file_hash: format!("{:016x}", axcenity_assets::content_hash(game)),
            files: pack.entries.len(),
            groups,
            largest,
            left_out_files: pack.left_out.len(),
            left_out_bytes: pack.left_out.iter().map(|(_, bytes)| bytes).sum(),
            problems: Vec::new(),
            uncooked: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// The report for people: what [`BUILD_REPORT_TEXT_FILE`] holds.
    pub fn to_text(&self) -> String {
        let mut text = String::new();
        let mut line = |line: String| {
            text.push_str(&line);
            text.push('\n');
        };
        line(format!("Build report: {}", self.project));
        line(format!(
            "Built {} with Axcenity {} for {} using the {} profile, in {:.1} s.",
            self.built_at, self.engine_version, self.target, self.profile, self.seconds
        ));
        line(self.settings.describe());
        if let Some(commit) = &self.project_commit {
            line(format!("Project commit: {commit}"));
        }
        line(String::new());
        line(format!(
            "game.axgame: {} in {} files (hash {}).",
            size_text(self.game_file_bytes),
            self.files,
            self.game_file_hash
        ));
        if self.left_out_files > 0 {
            line(format!(
                "Left out {} texture sources ({}) for their cooked copies.",
                self.left_out_files,
                size_text(self.left_out_bytes)
            ));
        }
        line(String::new());
        line("By kind (in the game file, then unpacked):".to_owned());
        for group in &self.groups {
            line(format!(
                "  {:<26} {:>6} files  {:>10}  {:>10}",
                group.name,
                group.files,
                size_text(group.stored),
                size_text(group.bytes)
            ));
        }
        line(String::new());
        line(format!("Largest {} files:", self.largest.len()));
        for file in &self.largest {
            line(format!("  {:>10}  {}", size_text(file.stored), file.path));
        }
        for (title, lines) in [
            ("Asset Check problems", &self.problems),
            ("Textures that ship as they are", &self.uncooked),
            ("Notes", &self.notes),
        ] {
            if lines.is_empty() {
                continue;
            }
            line(String::new());
            line(format!("{title} ({}):", lines.len()));
            for each in lines {
                line(format!("  {each}"));
            }
        }
        text
    }

    /// Writes the report into `destination` as JSON and as text.
    pub fn write(&self, destination: &Path) -> std::io::Result<()> {
        let mut json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        json.push(b'\n');
        fs::write(destination.join(BUILD_REPORT_FILE), json)?;
        fs::write(destination.join(BUILD_REPORT_TEXT_FILE), self.to_text())
    }
}

/// A size for people: bytes, KB or MB.
pub fn size_text(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{:.1} KB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

/// Now, in UTC, as `2026-11-12T23:10:05Z`.
fn utc_now() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    utc_text(seconds)
}

/// `seconds` since 1970 as a UTC date and time (Howard Hinnant's
/// days-to-civil, which needs no calendar crate).
fn utc_text(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest / 60 % 60,
        rest % 60
    )
}

/// The commit the project folder's Git checkout is on (12 digits), read
/// from `.git` without running Git. `None` outside a repository.
fn git_commit(folder: &Path) -> Option<String> {
    let git = folder.join(".git");
    let head = fs::read_to_string(git.join("HEAD")).ok()?;
    let head = head.trim();
    let full = match head.strip_prefix("ref: ") {
        Some(reference) => fs::read_to_string(git.join(reference))
            .ok()
            .map(|text| text.trim().to_owned())
            .or_else(|| {
                let packed = fs::read_to_string(git.join("packed-refs")).ok()?;
                packed.lines().find_map(|line| {
                    let (hash, name) = line.split_once(' ')?;
                    (name == reference).then(|| hash.to_owned())
                })
            })?,
        None => head.to_owned(),
    };
    (full.len() >= 12 && full.chars().all(|c| c.is_ascii_hexdigit())).then(|| full[..12].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-build-profiles-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn release_builds_as_build_game_always_has_and_a_project_adds_its_own() {
        let project = folder("profiles");
        let none = BuildProfiles::read(&project).unwrap();
        assert_eq!(none.names(), ["Release", "Development"]);
        let release = none.get(RELEASE_PROFILE).unwrap();
        assert_eq!(
            release.pack_options(),
            PackOptions {
                cooked_only: true,
                ..PackOptions::default()
            }
        );
        assert!(!none.get("Development").unwrap().cooked_only);

        fs::write(
            project.join(BUILD_PROFILES_FILE),
            r#"{ "format_version": 1, "profiles": {
                "Shipping": { "target": "windows", "compression_level": 9, "stop_on_problems": true },
                "Release": { "developer_console": false }
            } }"#,
        )
        .unwrap();
        let profiles = BuildProfiles::read(&project).unwrap();
        assert_eq!(profiles.names(), ["Release", "Development", "Shipping"]);
        let shipping = profiles.get("Shipping").unwrap();
        assert_eq!(shipping.target.as_deref(), Some("windows"));
        assert!(shipping.cooked_only, "on unless said");
        assert!(shipping.stop_on_problems);
        assert_eq!(
            profiles.get(RELEASE_PROFILE).unwrap().developer_console,
            Some(false),
            "the project's Release takes the built-in one's place"
        );
        assert_eq!(profiles.get("Nope"), None);

        fs::write(
            project.join(BUILD_PROFILES_FILE),
            r#"{ "profiles": { "Phone": { "target": "android" } } }"#,
        )
        .unwrap();
        let err = BuildProfiles::read(&project).unwrap_err();
        assert!(err.contains("android"), "{err}");
        let _ = fs::remove_dir_all(&project);
    }

    #[test]
    fn a_report_groups_the_game_by_kind_and_lists_the_largest_first() {
        let project = folder("report");
        fs::write(
            project.join(crate::MANIFEST_FILE),
            r#"{"name": "Crates", "format_version": 1}"#,
        )
        .unwrap();
        let entry = |name: &str, bytes: u64, stored: u64| crate::PackedEntry {
            name: name.to_owned(),
            bytes,
            stored,
        };
        let pack = PackReport {
            entries: vec![
                entry("axcenity.json", 100, 80),
                entry("scripts/Main.server.lua", 2_000, 900),
                entry(
                    ".axcenity/imported/textures/wood.png-1.axtex",
                    400_000,
                    380_000,
                ),
                entry("assets/textures/wood.png", 0, 0),
                entry("assets/models/crate.glb", 50_000, 30_000),
            ],
            left_out: vec![("assets/textures/wood.png".to_owned(), 1_500_000)],
        };
        let mut report = BuildReport::new(
            &project,
            RELEASE_PROFILE,
            &BuildProfile::default(),
            "linux",
            b"game",
            &pack,
        );
        assert_eq!(report.project, "Crates");
        assert_eq!(report.files, 5);
        assert_eq!(report.groups[0].name, "Cooked textures");
        assert_eq!(
            report.largest[0].path,
            ".axcenity/imported/textures/wood.png-1.axtex"
        );
        assert_eq!(
            (report.left_out_files, report.left_out_bytes),
            (1, 1_500_000)
        );
        assert_eq!(report.built_at.len(), "2026-11-12T23:10:05Z".len());
        report
            .problems
            .push("Workspace.Crate: `assets/x.png` isn't there".to_owned());
        let text = report.to_text();
        assert!(text.contains("using the Release profile"), "{text}");
        assert!(
            text.contains("Left out 1 texture sources (1.4 MB)"),
            "{text}"
        );
        assert!(text.contains("Asset Check problems (1):"), "{text}");

        report.write(&project).unwrap();
        let read: BuildReport =
            serde_json::from_slice(&fs::read(project.join(BUILD_REPORT_FILE)).unwrap()).unwrap();
        assert_eq!(read, report);
        let _ = fs::remove_dir_all(&project);
    }

    #[test]
    fn dates_and_commits_are_read_without_other_tools() {
        assert_eq!(utc_text(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc_text(1_762_989_005), "2025-11-12T23:10:05Z");
        assert_eq!(utc_text(951_782_400), "2000-02-29T00:00:00Z");

        let project = folder("git");
        assert_eq!(git_commit(&project), None);
        fs::create_dir_all(project.join(".git/refs/heads")).unwrap();
        fs::write(project.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(
            project.join(".git/packed-refs"),
            "# pack-refs\n0123456789abcdef0123456789abcdef01234567 refs/heads/main\n",
        )
        .unwrap();
        assert_eq!(git_commit(&project).as_deref(), Some("0123456789ab"));
        fs::write(
            project.join(".git/refs/heads/main"),
            "fedcba9876543210fedcba9876543210fedcba98\n",
        )
        .unwrap();
        assert_eq!(git_commit(&project).as_deref(), Some("fedcba987654"));
        let _ = fs::remove_dir_all(&project);
    }
}
