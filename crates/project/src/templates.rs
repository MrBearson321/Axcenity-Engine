use super::*;

/// What a new project starts with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Template {
    /// A grey baseplate to build on.
    #[default]
    Baseplate,
    /// Nothing at all.
    Empty,
    /// The genre templates (Phase 69): whole small games, each a folder in
    /// the repository's `templates/` with the shared pause and settings
    /// menu (`templates/shared/`) copied in.
    FirstPerson,
    ThirdPerson,
    Vehicle,
    TopDown,
    Arena,
}

/// What every genre template has too: the pause menu, the settings menu
/// with rebinding, and the save slot. A template's own file of the same
/// path is used instead.
const SHARED_FOLDER: &str = "shared";

// The `templates/` files, embedded by `build.rs`.
include!(concat!(env!("OUT_DIR"), "/template_files.rs"));

impl Template {
    pub const ALL: [Template; 7] = [
        Template::Baseplate,
        Template::Empty,
        Template::FirstPerson,
        Template::ThirdPerson,
        Template::Vehicle,
        Template::TopDown,
        Template::Arena,
    ];

    /// The templates that start from (almost) nothing. (The Starter scene
    /// went in unified Phase 92: the genre templates replace it.)
    pub const BLANK: [Template; 2] = [Template::Baseplate, Template::Empty];

    /// The playable genre templates.
    pub const GAMES: [Template; 5] = [
        Template::FirstPerson,
        Template::ThirdPerson,
        Template::Vehicle,
        Template::TopDown,
        Template::Arena,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Template::Baseplate => "Baseplate",
            Template::Empty => "Empty",
            Template::FirstPerson => "First Person",
            Template::ThirdPerson => "Third Person",
            Template::Vehicle => "Vehicle",
            Template::TopDown => "Top Down",
            Template::Arena => "Multiplayer Arena",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Template::Baseplate => "A grey baseplate to build on.",
            Template::Empty => "An empty world, for starting from nothing.",
            Template::FirstPerson => {
                "Shoot targets in first person with a blaster, with ammo on the HUD."
            }
            Template::ThirdPerson => {
                "Explore as a character: open a door, reach a checkpoint, save your coins."
            }
            Template::Vehicle => "Drive a car round a track against lap times from a data table.",
            Template::TopDown => "Click to move from above, past enemies run by a behavior tree.",
            Template::Arena => {
                "Red against blue for up to 8 players, with a leaderboard, chat and respawns."
            }
        }
    }

    /// The template's folder in `templates/`, for the genre templates.
    pub fn folder(self) -> Option<&'static str> {
        match self {
            Template::Baseplate | Template::Empty => None,
            Template::FirstPerson => Some("first-person"),
            Template::ThirdPerson => Some("third-person"),
            Template::Vehicle => Some("vehicle"),
            Template::TopDown => Some("top-down"),
            Template::Arena => Some("arena"),
        }
    }

    /// The template's picture for New Project (a PNG, 320 × 180), which
    /// also starts as the new project's `thumbnail.png`.
    pub fn picture(self) -> Option<&'static [u8]> {
        let folder = self.folder()?;
        TEMPLATE_FILES
            .iter()
            .find(|(template, path, _)| *template == folder && *path == THUMBNAIL_FILE)
            .map(|(_, _, bytes)| *bytes)
    }

    /// Every file a project made from this genre template starts with, by
    /// its path in the project: the shared files, then the template's own
    /// (which win). Empty for the others.
    pub fn files(self) -> Vec<(&'static str, &'static [u8])> {
        let Some(folder) = self.folder() else {
            return Vec::new();
        };
        let mut files: Vec<(&str, &[u8])> = Vec::new();
        for wanted in [SHARED_FOLDER, folder] {
            for (template, path, bytes) in TEMPLATE_FILES {
                if *template != wanted {
                    continue;
                }
                match files.iter_mut().find(|(have, _)| have == path) {
                    Some(file) => file.1 = bytes,
                    None => files.push((path, bytes)),
                }
            }
        }
        files
    }

    pub(super) fn world(self) -> &'static str {
        match self {
            Template::Baseplate => BASEPLATE_WORLD,
            Template::Empty => EMPTY_WORLD,
            // The genre templates have their own `world.json`.
            _ => EMPTY_WORLD,
        }
    }
}

// New worlds use Neutral tone mapping; a world saved without it is Classic.

pub(super) const EMPTY_WORLD: &str = r#"{
  "format_version": 1,
  "workspace": {},
  "lighting": {
    "properties": {
      "ToneMapping": "Neutral"
    }
  }
}
"#;

pub(super) const BASEPLATE_WORLD: &str = r#"{
  "format_version": 1,
  "workspace": {
    "children": [
      {
        "class": "Part",
        "name": "Baseplate",
        "properties": {
          "Anchored": true,
          "Locked": true,
          "Position": [0, -0.5, 0],
          "Size": [40, 1, 40],
          "Color": [0.35, 0.37, 0.4]
        }
      }
    ]
  },
  "lighting": {
    "properties": {
      "ToneMapping": "Neutral"
    }
  }
}
"#;

/// Makes a new project called `name` in a new folder inside `parent`, with a
/// baseplate in its world, and returns the folder. See [`create_project_from`].
pub fn create_project(parent: &Path, name: &str) -> Result<PathBuf, SaveError> {
    create_project_from(parent, name, Template::Baseplate)
}

/// Makes a new project called `name` from `template` in a new folder inside
/// `parent`, and returns the folder. The folder is named after the project
/// (characters filesystems don't allow become `-`). Fails (see
/// [`SaveError::already_exists`]) rather than touching a folder that exists.
pub fn create_project_from(
    parent: &Path,
    name: &str,
    template: Template,
) -> Result<PathBuf, SaveError> {
    let folder = make_project_folder(parent, name)?;
    let name = name.trim();
    let engine = axcenity_release::Version::current().to_string();
    let manifest = serde_json::json!({
        "name": name,
        "format_version": FORMAT_VERSION,
        "engine": { "created_with": engine, "saved_with": engine },
    });
    let manifest_text = serde_json::to_string_pretty(&manifest).unwrap_or_default();
    let mut written = fs::create_dir(folder.join(SCRIPTS_FOLDER))
        .map_err(SaveError::new(&folder))
        .and_then(|()| write_through_temporary(&folder.join(MANIFEST_FILE), &manifest_text))
        .and_then(|()| write_through_temporary(&folder.join(WORLD_FILE), template.world()));
    for (path, bytes) in template.files() {
        if written.is_err() {
            break;
        }
        let path = folder.join(path);
        written = match path.parent() {
            Some(parent) => fs::create_dir_all(parent).map_err(SaveError::new(parent)),
            None => Ok(()),
        }
        .and_then(|()| fs::write(&path, bytes).map_err(SaveError::new(&path)));
    }
    if let Err(err) = written {
        // Don't leave a half-made project behind.
        let _ = fs::remove_dir_all(&folder);
        return Err(err);
    }
    Ok(folder)
}

/// Makes the empty folder for a new project called `name` inside `parent`,
/// named after it (characters file systems don't allow become `-`), and
/// returns it. Fails (see [`SaveError::already_exists`]) rather than touch
/// a folder that exists.
pub(crate) fn make_project_folder(parent: &Path, name: &str) -> Result<PathBuf, SaveError> {
    let name = name.trim();
    let folder_name: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '-'
            } else {
                c
            }
        })
        .collect();
    let folder_name = folder_name.trim().trim_end_matches('.');
    let folder = parent.join(folder_name);
    if name.is_empty() || folder_name.is_empty() {
        return Err(SaveError::new(&folder)(std::io::Error::new(
            ErrorKind::InvalidInput,
            "the project needs a name",
        )));
    }
    // `create_dir` (not `_all`) fails if the folder is already there.
    fs::create_dir(&folder).map_err(SaveError::new(&folder))?;
    Ok(folder)
}
