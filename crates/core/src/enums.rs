//! Enum types, such as a part's shape or material. World files and scripts
//! refer to their items by name, for example `"Neon"` or `Enum.KeyCode.W`.

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

/// Which channel of a texture holds a grey map, such as a
/// SurfaceAppearance's `RoughnessChannel`. Packed textures keep several
/// maps in one image, one per channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureChannel {
    Red,
    Green,
    Blue,
    Alpha,
}

impl TextureChannel {
    pub const ALL: [TextureChannel; 4] = [
        TextureChannel::Red,
        TextureChannel::Green,
        TextureChannel::Blue,
        TextureChannel::Alpha,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            TextureChannel::Red => "Red",
            TextureChannel::Green => "Green",
            TextureChannel::Blue => "Blue",
            TextureChannel::Alpha => "Alpha",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.as_str() == name)
    }
}

/// The built-in picture a ParticleEmitter's particles show when it has no
/// `Texture` of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParticleType {
    /// A soft round dot.
    Circle,
    /// A bright point with a glow, for sparks and embers.
    Spark,
    /// A soft, cloudy puff.
    Smoke,
    /// A flickering flame shape.
    Fire,
    /// A five-pointed star.
    Star,
    /// A four-pointed twinkle.
    Magic,
}

impl ParticleType {
    pub const ALL: [ParticleType; 6] = [
        ParticleType::Circle,
        ParticleType::Spark,
        ParticleType::Smoke,
        ParticleType::Fire,
        ParticleType::Star,
        ParticleType::Magic,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ParticleType::Circle => "Circle",
            ParticleType::Spark => "Spark",
            ParticleType::Smoke => "Smoke",
            ParticleType::Fire => "Fire",
            ParticleType::Star => "Star",
            ParticleType::Magic => "Magic",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.as_str() == name)
    }
}

/// How Lighting turns the scene's high dynamic range light, where brightness
/// can go past 1, into colors a screen can show (`Lighting.ToneMapping`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToneMapping {
    /// Colors up to 0.6 pass unchanged and brighter ones roll off toward
    /// white, each channel on its own. The default, so worlds saved before
    /// tone mapping could be chosen look as they did.
    Classic,
    /// Keeps base colors close to their sRGB values and rolls highlights
    /// off without shifting their hue (Khronos PBR Neutral). New worlds use it.
    Neutral,
    /// A fitted ACES curve: more contrast, for a cinematic look.
    Filmic,
}

impl ToneMapping {
    pub const ALL: [ToneMapping; 3] = [
        ToneMapping::Classic,
        ToneMapping::Neutral,
        ToneMapping::Filmic,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ToneMapping::Classic => "Classic",
            ToneMapping::Neutral => "Neutral",
            ToneMapping::Filmic => "Filmic",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.as_str() == name)
    }
}

/// A key on the keyboard, for `UserInputService` in LocalScripts. Keys are
/// named by their place on a US keyboard, so `W` is the key above `S` on
/// every layout. Names follow Roblox's (`Return`, `LeftShift`, `Zero`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KeyCode {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Zero,
    One,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Space,
    Return,
    Escape,
    Tab,
    Backspace,
    Up,
    Down,
    Left,
    Right,
    LeftShift,
    RightShift,
    LeftControl,
    RightControl,
    LeftAlt,
    RightAlt,
}

impl KeyCode {
    pub const ALL: [KeyCode; 51] = [
        KeyCode::A,
        KeyCode::B,
        KeyCode::C,
        KeyCode::D,
        KeyCode::E,
        KeyCode::F,
        KeyCode::G,
        KeyCode::H,
        KeyCode::I,
        KeyCode::J,
        KeyCode::K,
        KeyCode::L,
        KeyCode::M,
        KeyCode::N,
        KeyCode::O,
        KeyCode::P,
        KeyCode::Q,
        KeyCode::R,
        KeyCode::S,
        KeyCode::T,
        KeyCode::U,
        KeyCode::V,
        KeyCode::W,
        KeyCode::X,
        KeyCode::Y,
        KeyCode::Z,
        KeyCode::Zero,
        KeyCode::One,
        KeyCode::Two,
        KeyCode::Three,
        KeyCode::Four,
        KeyCode::Five,
        KeyCode::Six,
        KeyCode::Seven,
        KeyCode::Eight,
        KeyCode::Nine,
        KeyCode::Space,
        KeyCode::Return,
        KeyCode::Escape,
        KeyCode::Tab,
        KeyCode::Backspace,
        KeyCode::Up,
        KeyCode::Down,
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::LeftShift,
        KeyCode::RightShift,
        KeyCode::LeftControl,
        KeyCode::RightControl,
        KeyCode::LeftAlt,
        KeyCode::RightAlt,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            KeyCode::A => "A",
            KeyCode::B => "B",
            KeyCode::C => "C",
            KeyCode::D => "D",
            KeyCode::E => "E",
            KeyCode::F => "F",
            KeyCode::G => "G",
            KeyCode::H => "H",
            KeyCode::I => "I",
            KeyCode::J => "J",
            KeyCode::K => "K",
            KeyCode::L => "L",
            KeyCode::M => "M",
            KeyCode::N => "N",
            KeyCode::O => "O",
            KeyCode::P => "P",
            KeyCode::Q => "Q",
            KeyCode::R => "R",
            KeyCode::S => "S",
            KeyCode::T => "T",
            KeyCode::U => "U",
            KeyCode::V => "V",
            KeyCode::W => "W",
            KeyCode::X => "X",
            KeyCode::Y => "Y",
            KeyCode::Z => "Z",
            KeyCode::Zero => "Zero",
            KeyCode::One => "One",
            KeyCode::Two => "Two",
            KeyCode::Three => "Three",
            KeyCode::Four => "Four",
            KeyCode::Five => "Five",
            KeyCode::Six => "Six",
            KeyCode::Seven => "Seven",
            KeyCode::Eight => "Eight",
            KeyCode::Nine => "Nine",
            KeyCode::Space => "Space",
            KeyCode::Return => "Return",
            KeyCode::Escape => "Escape",
            KeyCode::Tab => "Tab",
            KeyCode::Backspace => "Backspace",
            KeyCode::Up => "Up",
            KeyCode::Down => "Down",
            KeyCode::Left => "Left",
            KeyCode::Right => "Right",
            KeyCode::LeftShift => "LeftShift",
            KeyCode::RightShift => "RightShift",
            KeyCode::LeftControl => "LeftControl",
            KeyCode::RightControl => "RightControl",
            KeyCode::LeftAlt => "LeftAlt",
            KeyCode::RightAlt => "RightAlt",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.as_str() == name)
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
        for item in ToneMapping::ALL {
            assert_eq!(ToneMapping::from_name(item.as_str()), Some(item));
        }
        for item in KeyCode::ALL {
            assert_eq!(KeyCode::from_name(item.as_str()), Some(item));
        }
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
