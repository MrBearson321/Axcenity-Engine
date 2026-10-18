//! Enum property types, such as a part's shape or material. World files (and,
//! later, scripts) refer to their items by name, for example `"Neon"`.

/// The shape of a Part.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PartType {
    Block,
    /// A sphere. Its diameter is the smallest of the part's three sizes.
    Ball,
    /// Lies along the X axis: `Size.X` is its length, and its diameter is
    /// the smaller of `Size.Y` and `Size.Z`.
    Cylinder,
    /// A ramp. The bottom and back (+Z) faces are full rectangles, and the
    /// slope rises from the bottom front edge to the top back edge.
    Wedge,
}

impl PartType {
    pub const ALL: [PartType; 4] = [
        PartType::Block,
        PartType::Ball,
        PartType::Cylinder,
        PartType::Wedge,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PartType::Block => "Block",
            PartType::Ball => "Ball",
            PartType::Cylinder => "Cylinder",
            PartType::Wedge => "Wedge",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.as_str() == name)
    }
}

/// A material preset. Choosing one sets the part's `Metalness` and
/// `Roughness` to the preset's values, which can then be adjusted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Material {
    Plastic,
    Metal,
    /// See-through.
    Glass,
    /// Glows in the part's `Color`.
    Neon,
}

/// What a [`Material`] stands for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialPreset {
    /// Copied into the part's `Metalness` property when the material is chosen.
    pub metalness: f64,
    /// Copied into the part's `Roughness` property when the material is chosen.
    pub roughness: f64,
    /// How solid the surface looks, from 0 (invisible) to 1. Not a property:
    /// it always comes from the material.
    pub opacity: f32,
    /// True if the surface gives off its `Color` as light. Not a property.
    pub glows: bool,
}

impl Material {
    pub const ALL: [Material; 4] = [
        Material::Plastic,
        Material::Metal,
        Material::Glass,
        Material::Neon,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Material::Plastic => "Plastic",
            Material::Metal => "Metal",
            Material::Glass => "Glass",
            Material::Neon => "Neon",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.as_str() == name)
    }

    pub fn preset(self) -> MaterialPreset {
        let solid = MaterialPreset {
            metalness: 0.0,
            roughness: 0.5,
            opacity: 1.0,
            glows: false,
        };
        // `..solid` copies every field not listed from `solid`.
        match self {
            Material::Plastic => solid,
            Material::Metal => MaterialPreset {
                metalness: 1.0,
                roughness: 0.3,
                ..solid
            },
            Material::Glass => MaterialPreset {
                roughness: 0.05,
                opacity: 0.3,
                ..solid
            },
            Material::Neon => MaterialPreset {
                roughness: 1.0,
                glows: true,
                ..solid
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for item in PartType::ALL {
            assert_eq!(PartType::from_name(item.as_str()), Some(item));
        }
        for item in Material::ALL {
            assert_eq!(Material::from_name(item.as_str()), Some(item));
        }
        assert_eq!(Material::from_name("neon"), None);
    }

    #[test]
    fn only_glass_is_see_through_and_only_neon_glows() {
        for material in Material::ALL {
            let preset = material.preset();
            assert_eq!(preset.opacity < 1.0, material == Material::Glass);
            assert_eq!(preset.glows, material == Material::Neon);
        }
    }
}
