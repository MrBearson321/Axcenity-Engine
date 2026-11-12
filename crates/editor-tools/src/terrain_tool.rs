//! The Terrain Studio's tools: their names, icons, hints and keys. The
//! Terrain Studio itself is in the editor; the tool list lives here so the
//! shortcut table can name each tool.

use axcenity_editor_kit::theme::icons;
use eframe::egui;

/// One of the Terrain Studio's tools.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainTool {
    Raise,
    Smooth,
    Flatten,
    Sculpt,
    Stamp,
    Noise,
    Paint,
    Foliage,
    Auto,
    Generate,
    Erode,
    Masks,
}

impl TerrainTool {
    pub const SCULPTING: [TerrainTool; 6] = [
        TerrainTool::Raise,
        TerrainTool::Smooth,
        TerrainTool::Flatten,
        TerrainTool::Sculpt,
        TerrainTool::Stamp,
        TerrainTool::Noise,
    ];
    pub const MATERIALS: [TerrainTool; 3] =
        [TerrainTool::Paint, TerrainTool::Foliage, TerrainTool::Auto];
    pub const WHOLE: [TerrainTool; 3] = [
        TerrainTool::Generate,
        TerrainTool::Erode,
        TerrainTool::Masks,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TerrainTool::Raise => "Raise / Lower",
            TerrainTool::Smooth => "Smooth",
            TerrainTool::Flatten => "Flatten",
            TerrainTool::Sculpt => "Sculpt",
            TerrainTool::Stamp => "Stamp",
            TerrainTool::Noise => "Noise",
            TerrainTool::Paint => "Paint",
            TerrainTool::Foliage => "Foliage",
            TerrainTool::Auto => "Auto Materials",
            TerrainTool::Generate => "Generate",
            TerrainTool::Erode => "Erosion",
            TerrainTool::Masks => "Masks",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            TerrainTool::Raise => icons::ARROWS_DOWN_UP,
            TerrainTool::Smooth => icons::WAVES,
            TerrainTool::Flatten => icons::EQUALS,
            TerrainTool::Sculpt => icons::MOUNTAINS,
            TerrainTool::Stamp => icons::STAMP,
            TerrainTool::Noise => icons::DOTS_NINE,
            TerrainTool::Paint => icons::PAINT_BRUSH,
            TerrainTool::Foliage => icons::TREE,
            TerrainTool::Auto => icons::MAGIC_WAND,
            TerrainTool::Generate => icons::PLANET,
            TerrainTool::Erode => icons::CLOUD_RAIN,
            TerrainTool::Masks => icons::FUNNEL,
        }
    }

    /// Its key while the Terrain Studio has a tool, from the shortcut
    /// table: 1 to 9, then 0.
    pub fn key(self) -> Option<egui::Key> {
        crate::shortcuts::Action::TerrainTool(self)
            .keys()
            .first()
            .map(|chord| chord.logical_key)
    }

    pub fn all() -> impl Iterator<Item = TerrainTool> {
        Self::SCULPTING
            .into_iter()
            .chain(Self::MATERIALS)
            .chain(Self::WHOLE)
    }

    /// True for the tools used by brushing the terrain in the viewport.
    pub fn is_brush(self) -> bool {
        Self::SCULPTING.contains(&self) || matches!(self, TerrainTool::Paint | TerrainTool::Foliage)
    }

    /// One line on how to use it.
    pub fn hint(self) -> &'static str {
        match self {
            TerrainTool::Raise => "Drag to raise the ground. Hold Shift to lower it.",
            TerrainTool::Smooth => "Drag to even out bumps. Big shapes stay.",
            TerrainTool::Flatten => {
                "Drag to level the ground to the height where you start. Ctrl+click picks a height."
            }
            TerrainTool::Sculpt => "Drag to shape hills, ridges, valleys and cliffs.",
            TerrainTool::Stamp => "Click to stamp a shape. Shift stamps it upside down.",
            TerrainTool::Noise => "Drag to roughen the ground. Shift flips the bumps.",
            TerrainTool::Paint => {
                "Drag to paint the chosen layer. Shift erases back to the automatic materials. Ctrl+click picks a layer."
            }
            TerrainTool::Foliage => {
                "Drag to scatter the chosen plant or prop. Shift removes it. Density is how many per square stud."
            }
            TerrainTool::Auto => "Lay layers down by height and slope. Painting still wins.",
            TerrainTool::Generate => "Make a landscape from a preset and a seed.",
            TerrainTool::Erode => "Wear the landscape down with rain or gravity.",
            TerrainTool::Masks => "Limit every brush to some heights or slopes.",
        }
    }
}
