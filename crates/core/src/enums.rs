//! Enum types, such as a part's shape or material. World files and scripts
//! refer to their items by name, for example `"Neon"` or `Enum.KeyCode.W`.

use crate::value::PhysicalProperties;

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
    /// A cylinder with round ends, along the X axis like a Cylinder:
    /// `Size.X` is its whole length, ends included, and its diameter is the
    /// smaller of `Size.Y` and `Size.Z`. Ragdolls are made of these.
    Capsule,
}

impl PartType {
    pub const ALL: [PartType; 5] = [
        PartType::Block,
        PartType::Ball,
        PartType::Cylinder,
        PartType::Wedge,
        PartType::Capsule,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PartType::Block => "Block",
            PartType::Ball => "Ball",
            PartType::Cylinder => "Cylinder",
            PartType::Wedge => "Wedge",
            PartType::Capsule => "Capsule",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.as_str() == name)
    }
}

/// A material preset. Choosing one sets the part's `Metalness` and
/// `Roughness` to the preset's values, which can then be adjusted. Each
/// also has its own [`PhysicalProperties`] (see [`Material::physical`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Material {
    Plastic,
    /// Dense.
    Metal,
    /// See-through.
    Glass,
    /// Glows in the part's `Color`.
    Neon,
    /// Slippery.
    Ice,
    /// Grippy and bouncy.
    Rubber,
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
    pub const ALL: [Material; 6] = [
        Material::Plastic,
        Material::Metal,
        Material::Glass,
        Material::Neon,
        Material::Ice,
        Material::Rubber,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Material::Plastic => "Plastic",
            Material::Metal => "Metal",
            Material::Glass => "Glass",
            Material::Neon => "Neon",
            Material::Ice => "Ice",
            Material::Rubber => "Rubber",
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
            Material::Ice => MaterialPreset {
                roughness: 0.1,
                ..solid
            },
            Material::Rubber => MaterialPreset {
                roughness: 0.9,
                ..solid
            },
        }
    }

    /// How a part of this material weighs, grips and bounces while its
    /// `CustomPhysicalProperties` is nil. Plastic, Glass and Neon keep the
    /// values every part had before materials had their own (Phase 34), so
    /// worlds from then simulate the same; a project upgraded from then
    /// gives its Metal parts those values too.
    pub fn physical(self) -> PhysicalProperties {
        match self {
            Material::Plastic | Material::Glass | Material::Neon => PhysicalProperties::LEGACY,
            Material::Metal => PhysicalProperties::new(7.85, 0.4, 0.25, 1.0, 1.0),
            // The heavier weight makes ice slippery against anything.
            Material::Ice => PhysicalProperties::new(0.92, 0.02, 0.15, 3.0, 1.0),
            Material::Rubber => PhysicalProperties::new(1.3, 0.9, 0.85, 2.0, 3.0),
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
    // A game controller's buttons, with Xbox letters (South is A). The
    // sticks are inputs too: `Thumbstick1` (left) and `Thumbstick2` (right)
    // are their movement, `ButtonL3` and `ButtonR3` pressing them in.
    ButtonA,
    ButtonB,
    ButtonX,
    ButtonY,
    ButtonL1,
    ButtonR1,
    ButtonL2,
    ButtonR2,
    ButtonL3,
    ButtonR3,
    ButtonStart,
    ButtonSelect,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
    Thumbstick1,
    Thumbstick2,
    /// What `InputObject.KeyCode` is for input that isn't a key or a
    /// button, such as the mouse moving.
    Unknown,
}

impl KeyCode {
    pub const ALL: [KeyCode; 70] = [
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
        KeyCode::ButtonA,
        KeyCode::ButtonB,
        KeyCode::ButtonX,
        KeyCode::ButtonY,
        KeyCode::ButtonL1,
        KeyCode::ButtonR1,
        KeyCode::ButtonL2,
        KeyCode::ButtonR2,
        KeyCode::ButtonL3,
        KeyCode::ButtonR3,
        KeyCode::ButtonStart,
        KeyCode::ButtonSelect,
        KeyCode::DPadUp,
        KeyCode::DPadDown,
        KeyCode::DPadLeft,
        KeyCode::DPadRight,
        KeyCode::Thumbstick1,
        KeyCode::Thumbstick2,
        KeyCode::Unknown,
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
            KeyCode::ButtonA => "ButtonA",
            KeyCode::ButtonB => "ButtonB",
            KeyCode::ButtonX => "ButtonX",
            KeyCode::ButtonY => "ButtonY",
            KeyCode::ButtonL1 => "ButtonL1",
            KeyCode::ButtonR1 => "ButtonR1",
            KeyCode::ButtonL2 => "ButtonL2",
            KeyCode::ButtonR2 => "ButtonR2",
            KeyCode::ButtonL3 => "ButtonL3",
            KeyCode::ButtonR3 => "ButtonR3",
            KeyCode::ButtonStart => "ButtonStart",
            KeyCode::ButtonSelect => "ButtonSelect",
            KeyCode::DPadUp => "DPadUp",
            KeyCode::DPadDown => "DPadDown",
            KeyCode::DPadLeft => "DPadLeft",
            KeyCode::DPadRight => "DPadRight",
            KeyCode::Thumbstick1 => "Thumbstick1",
            KeyCode::Thumbstick2 => "Thumbstick2",
            KeyCode::Unknown => "Unknown",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.as_str() == name)
    }

    /// True for a game controller's buttons and sticks, false for the keyboard's keys.
    pub fn is_gamepad(self) -> bool {
        (KeyCode::ButtonA..=KeyCode::Thumbstick2).contains(&self)
    }

    /// True for `Thumbstick1` and `Thumbstick2`, which move rather than press.
    pub fn is_stick(self) -> bool {
        matches!(self, KeyCode::Thumbstick1 | KeyCode::Thumbstick2)
    }
}

/// What kind of device an input came from, for `InputObject.UserInputType`.
/// The mouse's buttons are kinds of their own, as in Roblox; game
/// controllers are numbered in the order they were plugged in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum UserInputType {
    Keyboard,
    MouseButton1,
    MouseButton2,
    MouseButton3,
    MouseMovement,
    MouseWheel,
    Touch,
    Gamepad1,
    Gamepad2,
    Gamepad3,
    Gamepad4,
    None,
}

impl UserInputType {
    pub const ALL: [UserInputType; 12] = [
        UserInputType::Keyboard,
        UserInputType::MouseButton1,
        UserInputType::MouseButton2,
        UserInputType::MouseButton3,
        UserInputType::MouseMovement,
        UserInputType::MouseWheel,
        UserInputType::Touch,
        UserInputType::Gamepad1,
        UserInputType::Gamepad2,
        UserInputType::Gamepad3,
        UserInputType::Gamepad4,
        UserInputType::None,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            UserInputType::Keyboard => "Keyboard",
            UserInputType::MouseButton1 => "MouseButton1",
            UserInputType::MouseButton2 => "MouseButton2",
            UserInputType::MouseButton3 => "MouseButton3",
            UserInputType::MouseMovement => "MouseMovement",
            UserInputType::MouseWheel => "MouseWheel",
            UserInputType::Touch => "Touch",
            UserInputType::Gamepad1 => "Gamepad1",
            UserInputType::Gamepad2 => "Gamepad2",
            UserInputType::Gamepad3 => "Gamepad3",
            UserInputType::Gamepad4 => "Gamepad4",
            UserInputType::None => "None",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.as_str() == name)
    }

    /// `Gamepad1` for controller 0, up to `Gamepad4`.
    pub fn gamepad(pad: u8) -> Option<Self> {
        [
            UserInputType::Gamepad1,
            UserInputType::Gamepad2,
            UserInputType::Gamepad3,
            UserInputType::Gamepad4,
        ]
        .get(usize::from(pad))
        .copied()
    }

    /// Which controller a `GamepadN` is, from 0.
    pub fn pad(self) -> Option<u8> {
        match self {
            UserInputType::Gamepad1 => Some(0),
            UserInputType::Gamepad2 => Some(1),
            UserInputType::Gamepad3 => Some(2),
            UserInputType::Gamepad4 => Some(3),
            _ => None,
        }
    }

    /// Which mouse button a `MouseButtonN` is, from 0 (left).
    pub fn mouse_button(self) -> Option<usize> {
        match self {
            UserInputType::MouseButton1 => Some(0),
            UserInputType::MouseButton2 => Some(1),
            UserInputType::MouseButton3 => Some(2),
            _ => None,
        }
    }
}

/// Where an input is in its life, for `InputObject.UserInputState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UserInputState {
    Begin,
    Change,
    End,
    Cancel,
    None,
}

impl UserInputState {
    pub const ALL: [UserInputState; 5] = [
        UserInputState::Begin,
        UserInputState::Change,
        UserInputState::End,
        UserInputState::Cancel,
        UserInputState::None,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            UserInputState::Begin => "Begin",
            UserInputState::Change => "Change",
            UserInputState::End => "End",
            UserInputState::Cancel => "Cancel",
            UserInputState::None => "None",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.as_str() == name)
    }
}

/// What the mouse cursor does, for `UserInputService.MouseBehavior`:
/// moves freely, or is held in the middle of the window, or where it was.
/// While held, moving the mouse still reports `GetMouseDelta`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MouseBehavior {
    #[default]
    Default,
    LockCenter,
    LockCurrentPosition,
}

impl MouseBehavior {
    pub const ALL: [MouseBehavior; 3] = [
        MouseBehavior::Default,
        MouseBehavior::LockCenter,
        MouseBehavior::LockCurrentPosition,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            MouseBehavior::Default => "Default",
            MouseBehavior::LockCenter => "LockCenter",
            MouseBehavior::LockCurrentPosition => "LockCurrentPosition",
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
            let name = item.as_str();
            let pad = ["Button", "DPad", "Thumbstick"]
                .iter()
                .any(|start| name.starts_with(start));
            assert_eq!(item.is_gamepad(), pad, "{item:?}");
        }
        for item in UserInputType::ALL {
            assert_eq!(UserInputType::from_name(item.as_str()), Some(item));
        }
        for item in UserInputState::ALL {
            assert_eq!(UserInputState::from_name(item.as_str()), Some(item));
        }
        for item in MouseBehavior::ALL {
            assert_eq!(MouseBehavior::from_name(item.as_str()), Some(item));
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
