//! The Assets panel's tiles as other panels see them: what kind of asset a
//! file is ([`AssetKind`]), a tile ([`AssetEntry`]), what's dragged from the
//! panel ([`AssetPayload`]) and a texture for a picker ([`TexturePick`]).
//! The panel itself is the editor's `asset_browser.rs`.

use axcenity_core::FoliageSource;
use eframe::egui;

use crate::props::{PROPS, Prop};
use crate::theme::{self, icons};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AssetKind {
    Prop,
    Model,
    Prefab,
    Image,
    Texture,
    /// A texture set (`*.texture.json`) and the maps beside it.
    TextureSet,
    /// A material file (`*.material.json`).
    Material,
    Audio,
    Particle,
    /// An animation clip (`*.anim.json`).
    Animation,
    /// A state machine (`*.statemachine.json`).
    Controller,
    /// A navigation agent's behavior tree (`*.behaviortree.json`).
    BehaviorTree,
    /// A data table scripts read (`assets/data/*.data.json`).
    DataTable,
    /// A cutscene sequence (`*.sequence.json`, Phase 51).
    Sequence,
    /// A sky image (`.exr` or `.hdr`) for Lighting's `SkyImage`.
    Sky,
}

impl AssetKind {
    pub fn icon(self) -> &'static str {
        match self {
            AssetKind::Prop => icons::SHAPES,
            AssetKind::Model => icons::CUBE_TRANSPARENT,
            AssetKind::Prefab => icons::PACKAGE,
            AssetKind::Image => icons::IMAGE,
            AssetKind::Texture | AssetKind::TextureSet => icons::IMAGE_SQUARE,
            AssetKind::Material => icons::SPHERE,
            AssetKind::Audio => icons::WAVEFORM,
            AssetKind::Particle => icons::SPARKLE,
            AssetKind::Animation => theme::AnimKind::Clip.icon(),
            AssetKind::Controller => theme::AnimKind::StateMachine.icon(),
            AssetKind::BehaviorTree => icons::BRAIN,
            AssetKind::DataTable => icons::TABLE,
            AssetKind::Sequence => icons::FILM_SLATE,
            AssetKind::Sky => icons::SUN_HORIZON,
        }
    }

    pub fn color(self, visuals: &egui::Visuals) -> egui::Color32 {
        use axcenity_core::ClassName;
        match self {
            AssetKind::Prop | AssetKind::Model | AssetKind::Prefab => {
                theme::class_color(ClassName::Part, visuals)
            }
            AssetKind::Image | AssetKind::Texture | AssetKind::TextureSet | AssetKind::Material => {
                theme::class_color(ClassName::SurfaceAppearance, visuals)
            }
            AssetKind::Audio => theme::class_color(ClassName::Sound, visuals),
            AssetKind::Particle => theme::class_color(ClassName::PointLight, visuals),
            AssetKind::Animation => theme::AnimKind::Clip.color(visuals),
            AssetKind::Controller => theme::AnimKind::StateMachine.color(visuals),
            AssetKind::BehaviorTree => theme::class_color(ClassName::NavigationAgent, visuals),
            AssetKind::DataTable => theme::class_color(ClassName::Script, visuals),
            AssetKind::Sequence => theme::AnimKind::Clip.color(visuals),
            AssetKind::Sky => theme::class_color(ClassName::Lighting, visuals),
        }
    }

    /// What it is, in a list row.
    pub fn label(self) -> &'static str {
        match self {
            AssetKind::Prop => "Prop",
            AssetKind::Model => "Model",
            AssetKind::Prefab => "Prefab",
            AssetKind::Image => "Image",
            AssetKind::Texture => "Texture",
            AssetKind::TextureSet => "Texture set",
            AssetKind::Material => "Material",
            AssetKind::Audio => "Sound",
            AssetKind::Particle => "Particle",
            AssetKind::Animation => "Clip",
            AssetKind::Controller => "State machine",
            AssetKind::BehaviorTree => "Behavior tree",
            AssetKind::DataTable => "Data table",
            AssetKind::Sequence => "Sequence",
            AssetKind::Sky => "Sky",
        }
    }

    /// What dropping one in the viewport does, for its tile's tooltip.
    pub fn hover(self) -> &'static str {
        match self {
            AssetKind::Prop | AssetKind::Prefab => {
                "Drag into the viewport to place a copy on the surface under the mouse."
            }
            AssetKind::Model => "Drag into the viewport to place it as a MeshPart.",
            AssetKind::Image => {
                "Drag onto an ImageLabel to show it there, or anywhere else to add an ImageLabel."
            }
            AssetKind::Texture => {
                "Drag onto a MeshPart to use it as the color map of its SurfaceAppearances."
            }
            AssetKind::TextureSet => {
                "Drag onto a part, MeshPart, or terrain (or a layer in the Terrain Studio) to \
                 fill its maps, or double-click to fill the selection."
            }
            AssetKind::Material => {
                "Drag onto a part or MeshPart to draw it in this material (its MaterialVariant), \
                 or double-click to use it on the selection."
            }
            AssetKind::Audio => {
                "Drag onto a part to put a Sound in it, or anywhere else for one in the Workspace."
            }
            AssetKind::Particle => {
                "Drag onto a part to give it a ParticleEmitter with this texture."
            }
            AssetKind::Animation => "Double-click to open it in the Animation tab.",
            AssetKind::Controller => "Drag onto a part to give its Animator this state machine.",
            AssetKind::BehaviorTree => {
                "Drag onto an agent's body to have it run this tree in Play, or double-click to \
                 open it in the Behavior Tree tab."
            }
            AssetKind::DataTable => {
                "Double-click to open it in the Data Table tab. Scripts read it with \
                 DataService:GetTable."
            }
            AssetKind::Sequence => {
                "Double-click to open it in the Sequencer tab. A Sequence Instance plays it."
            }
            AssetKind::Sky => {
                "Double-click, or drag into the viewport or onto Lighting's SkyImage, to use it \
                 as the sky. You're asked first."
            }
        }
    }
}

/// A tile in the panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetEntry {
    pub kind: AssetKind,
    /// The file's path in the project, with `/` between folders, or a
    /// prop's name.
    pub path: String,
    /// What the tile says: the file name without its extension.
    pub name: String,
}

impl AssetEntry {
    /// The built-in prop this tile is, if it is one.
    pub fn prop(&self) -> Option<&'static Prop> {
        (self.kind == AssetKind::Prop)
            .then(|| PROPS.iter().find(|prop| prop.name == self.path))
            .flatten()
    }

    /// The foliage this tile paints, if the Foliage tool can use it.
    pub fn foliage_source(&self) -> Option<FoliageSource> {
        match self.kind {
            AssetKind::Prop => {
                axcenity_core::FoliageProp::from_name(&self.path).map(FoliageSource::Prop)
            }
            AssetKind::Model => Some(FoliageSource::Model(self.path.clone())),
            _ => None,
        }
    }
}

/// What's dragged from the panel to the viewport (or to a terrain layer).
#[derive(Clone, Debug)]
pub struct AssetPayload(pub AssetEntry);

/// A texture set or texture in the project, for a picker outside the panel.
#[derive(Clone)]
pub struct TexturePick {
    pub entry: AssetEntry,
    /// `None` until it has been read, or if it can't be.
    pub thumbnail: Option<egui::TextureHandle>,
}
