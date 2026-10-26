use super::*;

/// What a new project starts with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Template {
    /// A grey baseplate to build on.
    #[default]
    Baseplate,
    /// Nothing at all.
    Empty,
    /// A small lit scene: a baseplate, a glowing part that spins, a pad that
    /// changes colour when touched, and the block script doing both.
    Starter,
}

impl Template {
    pub const ALL: [Template; 3] = [Template::Baseplate, Template::Empty, Template::Starter];

    pub fn name(self) -> &'static str {
        match self {
            Template::Baseplate => "Baseplate",
            Template::Empty => "Empty",
            Template::Starter => "Starter scene",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Template::Baseplate => "A grey baseplate to build on.",
            Template::Empty => "An empty world, for starting from nothing.",
            Template::Starter => {
                "A lit scene with a spinning neon part and a touch pad, run by blocks."
            }
        }
    }

    pub(super) fn world(self) -> &'static str {
        match self {
            Template::Baseplate => BASEPLATE_WORLD,
            Template::Empty => EMPTY_WORLD,
            Template::Starter => STARTER_WORLD,
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

pub(super) const STARTER_WORLD: &str = r#"{
  "format_version": 1,
  "workspace": {
    "children": [
      {
        "class": "Part",
        "name": "Baseplate",
        "properties": {
          "Anchored": true,
          "Position": [0, -0.5, 0],
          "Size": [60, 1, 60],
          "Color": [0.35, 0.37, 0.4]
        }
      },
      {
        "class": "Part",
        "name": "Spinner",
        "properties": {
          "Anchored": true,
          "Position": [0, 5, 0],
          "Size": [6, 1, 1],
          "Color": [0.3, 0.75, 1],
          "Material": "Neon",
          "CastShadow": false
        },
        "children": [
          {
            "class": "PointLight",
            "name": "Glow",
            "properties": {
              "Color": [0.3, 0.75, 1],
              "Brightness": 2,
              "Range": 18
            }
          }
        ]
      },
      {
        "class": "Part",
        "name": "Pillar",
        "properties": {
          "Anchored": true,
          "Position": [-8, 3, -4],
          "Size": [2, 6, 2],
          "Color": [0.85, 0.8, 0.7]
        }
      },
      {
        "class": "Part",
        "name": "Pad",
        "properties": {
          "Anchored": true,
          "Position": [8, 0.25, 4],
          "Size": [6, 0.5, 6],
          "Color": [0.95, 0.65, 0.2]
        }
      },
      {
        "class": "Part",
        "name": "Ball",
        "properties": {
          "Position": [8, 10, 4],
          "Shape": "Ball",
          "Size": [2, 2, 2],
          "Color": [0.9, 0.3, 0.35]
        }
      }
    ]
  },
  "lighting": {
    "properties": {
      "ShadowSoftness": 0.35,
      "ToneMapping": "Neutral"
    }
  }
}
"#;

/// The block script of the starter scene: spin the neon part, and turn the
/// pad green when something lands on it.
pub(super) fn starter_blocks() -> BlockScript {
    use axcenity_blocks::Expression;
    let vector = |x, y, z| Expression::Vector3 { x, y, z };
    BlockScript {
        blocks: vec![
            Block::new(BlockKind::WhenStarted {
                body: vec![Block::new(BlockKind::Forever {
                    body: vec![Block::new(BlockKind::TurnBy {
                        target: "Spinner".to_owned(),
                        rotation: vector(0.0, 2.0, 0.0),
                    })],
                })],
            }),
            Block::new(BlockKind::WhenTouched {
                part: "Pad".to_owned(),
                body: vec![
                    Block::new(BlockKind::SetColor {
                        target: "Pad".to_owned(),
                        color: Expression::Color3 {
                            r: 0.3,
                            g: 0.85,
                            b: 0.45,
                        },
                    }),
                    Block::new(BlockKind::Print {
                        value: Expression::Text {
                            value: "Something landed on the pad!".to_owned(),
                        },
                    }),
                ],
            }),
        ],
        ..Default::default()
    }
}

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
    let manifest = serde_json::json!({ "name": name, "format_version": FORMAT_VERSION });
    let manifest_text = serde_json::to_string_pretty(&manifest).unwrap_or_default();
    let mut written = fs::create_dir(folder.join(SCRIPTS_FOLDER))
        .map_err(SaveError::new(&folder))
        .and_then(|()| write_through_temporary(&folder.join(MANIFEST_FILE), &manifest_text))
        .and_then(|()| write_through_temporary(&folder.join(WORLD_FILE), template.world()));
    if template == Template::Starter && written.is_ok() {
        let path = folder
            .join(SCRIPTS_FOLDER)
            .join("Starter.server.blocks.json");
        let blocks = serde_json::to_string_pretty(&starter_blocks()).unwrap_or_default();
        written = write_through_temporary(&path, &blocks);
    }
    if let Err(err) = written {
        // Don't leave a half-made project behind.
        let _ = fs::remove_dir_all(&folder);
        return Err(err);
    }
    Ok(folder)
}
