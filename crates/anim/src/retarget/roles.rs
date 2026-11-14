//! Humanoid roles, and which bone of a skeleton plays each: matched by
//! name, or set by a rig definition (`assets::RigDefinition`).

use axcenity_assets::{Bone, RigDefinition, RigDefinitions};

/// A side of the body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
}

/// A finger (the thumb counts as one).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Digit {
    Thumb,
    Index,
    Middle,
    Ring,
    Little,
}

/// A finger's bone, from the hand out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Segment {
    Proximal,
    Intermediate,
    Distal,
}

impl Digit {
    pub const ALL: [Digit; 5] = [
        Digit::Thumb,
        Digit::Index,
        Digit::Middle,
        Digit::Ring,
        Digit::Little,
    ];
}

impl Segment {
    pub const ALL: [Segment; 3] = [Segment::Proximal, Segment::Intermediate, Segment::Distal];
}

/// A role a bone of a humanoid skeleton plays: one of the body's 22, or a
/// finger bone (three per finger, five fingers a hand). The names are the
/// ones Unity and VRM use (`LeftUpperArm`, `RightIndexProximal`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HumanBone {
    Hips,
    Spine,
    Chest,
    UpperChest,
    Neck,
    Head,
    LeftShoulder,
    LeftUpperArm,
    LeftLowerArm,
    LeftHand,
    RightShoulder,
    RightUpperArm,
    RightLowerArm,
    RightHand,
    LeftUpperLeg,
    LeftLowerLeg,
    LeftFoot,
    LeftToes,
    RightUpperLeg,
    RightLowerLeg,
    RightFoot,
    RightToes,
    Finger(Side, Digit, Segment),
}

/// The body's roles, in [`HumanBone::index`] order.
const BODY: [HumanBone; 22] = [
    HumanBone::Hips,
    HumanBone::Spine,
    HumanBone::Chest,
    HumanBone::UpperChest,
    HumanBone::Neck,
    HumanBone::Head,
    HumanBone::LeftShoulder,
    HumanBone::LeftUpperArm,
    HumanBone::LeftLowerArm,
    HumanBone::LeftHand,
    HumanBone::RightShoulder,
    HumanBone::RightUpperArm,
    HumanBone::RightLowerArm,
    HumanBone::RightHand,
    HumanBone::LeftUpperLeg,
    HumanBone::LeftLowerLeg,
    HumanBone::LeftFoot,
    HumanBone::LeftToes,
    HumanBone::RightUpperLeg,
    HumanBone::RightLowerLeg,
    HumanBone::RightFoot,
    HumanBone::RightToes,
];

const BODY_NAMES: [&str; 22] = [
    "Hips",
    "Spine",
    "Chest",
    "UpperChest",
    "Neck",
    "Head",
    "LeftShoulder",
    "LeftUpperArm",
    "LeftLowerArm",
    "LeftHand",
    "RightShoulder",
    "RightUpperArm",
    "RightLowerArm",
    "RightHand",
    "LeftUpperLeg",
    "LeftLowerLeg",
    "LeftFoot",
    "LeftToes",
    "RightUpperLeg",
    "RightLowerLeg",
    "RightFoot",
    "RightToes",
];

/// The finger roles' names, in [`HumanBone::index`] order (left hand first,
/// thumb to little finger, each from the hand out).
const FINGER_NAMES: [&str; 30] = [
    "LeftThumbProximal",
    "LeftThumbIntermediate",
    "LeftThumbDistal",
    "LeftIndexProximal",
    "LeftIndexIntermediate",
    "LeftIndexDistal",
    "LeftMiddleProximal",
    "LeftMiddleIntermediate",
    "LeftMiddleDistal",
    "LeftRingProximal",
    "LeftRingIntermediate",
    "LeftRingDistal",
    "LeftLittleProximal",
    "LeftLittleIntermediate",
    "LeftLittleDistal",
    "RightThumbProximal",
    "RightThumbIntermediate",
    "RightThumbDistal",
    "RightIndexProximal",
    "RightIndexIntermediate",
    "RightIndexDistal",
    "RightMiddleProximal",
    "RightMiddleIntermediate",
    "RightMiddleDistal",
    "RightRingProximal",
    "RightRingIntermediate",
    "RightRingDistal",
    "RightLittleProximal",
    "RightLittleIntermediate",
    "RightLittleDistal",
];

/// How many roles there are.
pub const ROLE_COUNT: usize = 52;

impl HumanBone {
    /// The roles a skeleton needs to be retargeted to or from.
    pub const REQUIRED: [HumanBone; 15] = [
        HumanBone::Hips,
        HumanBone::Spine,
        HumanBone::Head,
        HumanBone::LeftUpperArm,
        HumanBone::LeftLowerArm,
        HumanBone::LeftHand,
        HumanBone::RightUpperArm,
        HumanBone::RightLowerArm,
        HumanBone::RightHand,
        HumanBone::LeftUpperLeg,
        HumanBone::LeftLowerLeg,
        HumanBone::LeftFoot,
        HumanBone::RightUpperLeg,
        HumanBone::RightLowerLeg,
        HumanBone::RightFoot,
    ];

    /// Every role: the body's, then the fingers'.
    pub fn all() -> impl Iterator<Item = HumanBone> {
        (0..ROLE_COUNT).map(HumanBone::from_index)
    }

    /// The finger roles of one hand, thumb first, each from the hand out.
    pub fn fingers(side: Side) -> impl Iterator<Item = HumanBone> {
        Digit::ALL.into_iter().flat_map(move |digit| {
            Segment::ALL
                .into_iter()
                .map(move |segment| HumanBone::Finger(side, digit, segment))
        })
    }

    /// Its place in [`all`](Self::all).
    pub fn index(self) -> usize {
        match self {
            HumanBone::Finger(side, digit, segment) => {
                let side = match side {
                    Side::Left => 0,
                    Side::Right => 1,
                };
                let digit = Digit::ALL.iter().position(|&d| d == digit).expect("listed");
                let segment = Segment::ALL
                    .iter()
                    .position(|&s| s == segment)
                    .expect("listed");
                BODY.len() + side * 15 + digit * 3 + segment
            }
            body => BODY.iter().position(|&role| role == body).expect("listed"),
        }
    }

    fn from_index(index: usize) -> HumanBone {
        if let Some(&body) = BODY.get(index) {
            return body;
        }
        let finger = index - BODY.len();
        let side = if finger < 15 { Side::Left } else { Side::Right };
        HumanBone::Finger(side, Digit::ALL[finger % 15 / 3], Segment::ALL[finger % 3])
    }

    /// Its name, as messages and rig definitions write it.
    pub fn name(self) -> &'static str {
        let index = self.index();
        BODY_NAMES
            .get(index)
            .copied()
            .unwrap_or_else(|| FINGER_NAMES[index - BODY.len()])
    }

    /// The role named `name` (as [`name`](Self::name) writes it, any case).
    pub fn from_name(name: &str) -> Option<HumanBone> {
        HumanBone::all().find(|role| role.name().eq_ignore_ascii_case(name))
    }

    /// Whether it's a finger's bone.
    pub fn is_finger(self) -> bool {
        matches!(self, HumanBone::Finger(..))
    }

    /// The role whose bone should be its bone's parent or further above
    /// it, for checking a skeleton's hierarchy: the next role in towards
    /// the hips.
    pub fn above(self) -> Option<HumanBone> {
        use HumanBone::*;
        Some(match self {
            Hips => return None,
            Spine | LeftUpperLeg | RightUpperLeg => Hips,
            Chest => Spine,
            UpperChest => Chest,
            Neck => Spine,
            Head => Neck,
            LeftShoulder | RightShoulder => Spine,
            LeftUpperArm => LeftShoulder,
            RightUpperArm => RightShoulder,
            LeftLowerArm => LeftUpperArm,
            RightLowerArm => RightUpperArm,
            LeftHand => LeftLowerArm,
            RightHand => RightLowerArm,
            LeftLowerLeg => LeftUpperLeg,
            RightLowerLeg => RightUpperLeg,
            LeftFoot => LeftLowerLeg,
            RightFoot => RightLowerLeg,
            LeftToes => LeftFoot,
            RightToes => RightFoot,
            Finger(side, digit, segment) => match segment {
                Segment::Proximal => HumanBone::hand(side),
                Segment::Intermediate => Finger(side, digit, Segment::Proximal),
                Segment::Distal => Finger(side, digit, Segment::Intermediate),
            },
        })
    }

    /// The roles whose joint a bone in this role points at when it rests
    /// (the first a skeleton has wins), which says which way the bone
    /// lies: the next role out from the hips.
    pub fn toward(self) -> Vec<HumanBone> {
        use HumanBone::*;
        let proximal = |side, digit| Finger(side, digit, Segment::Proximal);
        match self {
            Hips => vec![Spine],
            Spine => vec![Chest, UpperChest, Neck],
            Chest => vec![UpperChest, Neck],
            UpperChest => vec![Neck],
            Neck => vec![Head],
            LeftShoulder => vec![LeftUpperArm],
            RightShoulder => vec![RightUpperArm],
            LeftUpperArm => vec![LeftLowerArm],
            RightUpperArm => vec![RightLowerArm],
            LeftLowerArm => vec![LeftHand],
            RightLowerArm => vec![RightHand],
            LeftHand => vec![
                proximal(Side::Left, Digit::Middle),
                proximal(Side::Left, Digit::Index),
            ],
            RightHand => vec![
                proximal(Side::Right, Digit::Middle),
                proximal(Side::Right, Digit::Index),
            ],
            LeftUpperLeg => vec![LeftLowerLeg],
            RightUpperLeg => vec![RightLowerLeg],
            LeftLowerLeg => vec![LeftFoot],
            RightLowerLeg => vec![RightFoot],
            LeftFoot => vec![LeftToes],
            RightFoot => vec![RightToes],
            Finger(side, digit, Segment::Proximal) => {
                vec![Finger(side, digit, Segment::Intermediate)]
            }
            Finger(side, digit, Segment::Intermediate) => {
                vec![Finger(side, digit, Segment::Distal)]
            }
            Head | LeftToes | RightToes | Finger(_, _, Segment::Distal) => Vec::new(),
        }
    }

    /// The hand on `side`.
    pub fn hand(side: Side) -> HumanBone {
        match side {
            Side::Left => HumanBone::LeftHand,
            Side::Right => HumanBone::RightHand,
        }
    }

    /// The same role on the other side of the body (itself in the middle).
    pub fn mirrored(self) -> HumanBone {
        use HumanBone::*;
        match self {
            LeftShoulder => RightShoulder,
            LeftUpperArm => RightUpperArm,
            LeftLowerArm => RightLowerArm,
            LeftHand => RightHand,
            RightShoulder => LeftShoulder,
            RightUpperArm => LeftUpperArm,
            RightLowerArm => LeftLowerArm,
            RightHand => LeftHand,
            LeftUpperLeg => RightUpperLeg,
            LeftLowerLeg => RightLowerLeg,
            LeftFoot => RightFoot,
            LeftToes => RightToes,
            RightUpperLeg => LeftUpperLeg,
            RightLowerLeg => LeftLowerLeg,
            RightFoot => LeftFoot,
            RightToes => LeftToes,
            Finger(Side::Left, digit, segment) => Finger(Side::Right, digit, segment),
            Finger(Side::Right, digit, segment) => Finger(Side::Left, digit, segment),
            middle => middle,
        }
    }
}

/// What kind of bone a name is, before sides and the spine are sorted out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Hips,
    Spine,
    Neck,
    Head,
    Shoulder,
    UpperArm,
    LowerArm,
    Hand,
    UpperLeg,
    LowerLeg,
    Foot,
    Toes,
    Finger(Digit, Segment),
}

/// Which bone of a skeleton plays each [`HumanBone`] role.
#[derive(Clone, Debug, PartialEq)]
pub struct HumanoidMap {
    bones: [Option<usize>; ROLE_COUNT],
}

impl Default for HumanoidMap {
    fn default() -> Self {
        Self {
            bones: [None; ROLE_COUNT],
        }
    }
}

impl HumanoidMap {
    /// Matches `bones` (parents first) to roles by their names. The first
    /// bone of a kind and side wins; spine bones between the hips and the
    /// neck become Spine, then Chest, and the last of three or more
    /// UpperChest.
    pub fn detect(bones: &[Bone]) -> HumanoidMap {
        let mut map = HumanoidMap::default();
        let mut spine = Vec::new();
        for (index, bone) in bones.iter().enumerate() {
            let Some((kind, side)) = classify(&bone.name) else {
                continue;
            };
            let role = match (kind, side) {
                (Kind::Spine, _) => {
                    spine.push(index);
                    continue;
                }
                (Kind::Hips, _) => HumanBone::Hips,
                (Kind::Neck, _) => HumanBone::Neck,
                (Kind::Head, _) => HumanBone::Head,
                (_, None) => continue,
                (Kind::Finger(digit, segment), Some(side)) => {
                    HumanBone::Finger(side, digit, segment)
                }
                (kind, Some(side)) => {
                    let left = match kind {
                        Kind::Shoulder => HumanBone::LeftShoulder,
                        Kind::UpperArm => HumanBone::LeftUpperArm,
                        Kind::LowerArm => HumanBone::LeftLowerArm,
                        Kind::Hand => HumanBone::LeftHand,
                        Kind::UpperLeg => HumanBone::LeftUpperLeg,
                        Kind::LowerLeg => HumanBone::LeftLowerLeg,
                        Kind::Foot => HumanBone::LeftFoot,
                        _ => HumanBone::LeftToes,
                    };
                    match side {
                        Side::Left => left,
                        Side::Right => left.mirrored(),
                    }
                }
            };
            map.bones[role.index()].get_or_insert(index);
        }
        let roles: &[HumanBone] = match spine.len() {
            0 => &[],
            1 => &[HumanBone::Spine],
            2 => &[HumanBone::Spine, HumanBone::Chest],
            _ => &[HumanBone::Spine, HumanBone::Chest, HumanBone::UpperChest],
        };
        // Spine, the next as Chest, the last as UpperChest.
        let picks = match spine.len() {
            0..=2 => spine.clone(),
            _ => vec![spine[0], spine[1], spine[spine.len() - 1]],
        };
        for (&role, bone) in roles.iter().zip(picks) {
            map.bones[role.index()] = Some(bone);
        }
        map
    }

    /// The map for `bones` with `definition` over what names match: each
    /// role it sets plays the bone it names (or none, for `""`), and a
    /// role it leaves out keeps the bone matched by name, unless the
    /// definition gave that bone another role. Role names it doesn't know
    /// and bones the skeleton doesn't have are left out
    /// (`check_rig` reports them).
    pub fn with_definition(bones: &[Bone], definition: &RigDefinition) -> HumanoidMap {
        let mut map = HumanoidMap::detect(bones);
        let mut given = Vec::new();
        for (role, bone) in &definition.bones {
            let Some(role) = HumanBone::from_name(role) else {
                continue;
            };
            let found = if bone.is_empty() {
                None
            } else {
                bones.iter().position(|b| &b.name == bone)
            };
            map.bones[role.index()] = found;
            given.push(role);
            if let Some(found) = found {
                for other in HumanBone::all() {
                    if map.bones[other.index()] == Some(found) && !given.contains(&other) {
                        map.bones[other.index()] = None;
                    }
                }
            }
        }
        // A role given later can't take back a bone an earlier one claimed,
        // so clear name matches that clash with any given role.
        for role in HumanBone::all() {
            if given.contains(&role) {
                continue;
            }
            if let Some(bone) = map.bones[role.index()] {
                let taken = given.iter().any(|&g| map.bones[g.index()] == Some(bone));
                if taken {
                    map.bones[role.index()] = None;
                }
            }
        }
        map
    }

    /// The map for `bones`: the project's best-fitting rig definition's
    /// (see `RigDefinitions::for_skeleton`), with its path, or else by
    /// names alone.
    pub fn for_skeleton(bones: &[Bone], rigs: &RigDefinitions) -> (HumanoidMap, Option<String>) {
        match rigs.for_skeleton(bones.iter().map(|bone| bone.name.as_str())) {
            Some((path, definition)) => (
                HumanoidMap::with_definition(bones, definition),
                Some(path.to_owned()),
            ),
            None => (HumanoidMap::detect(bones), None),
        }
    }

    /// The bone playing `role`, if any.
    pub fn get(&self, role: HumanBone) -> Option<usize> {
        self.bones[role.index()]
    }

    /// Sets (or, with `None`, clears) the bone playing `role`.
    pub fn set(&mut self, role: HumanBone, bone: Option<usize>) {
        self.bones[role.index()] = bone;
    }

    /// Every role that has a bone, with it.
    pub fn roles(&self) -> impl Iterator<Item = (HumanBone, usize)> + '_ {
        HumanBone::all().filter_map(|role| Some((role, self.get(role)?)))
    }

    /// Every bone that plays a role.
    pub fn bones(&self) -> impl Iterator<Item = usize> + '_ {
        self.bones.iter().flatten().copied()
    }

    /// The [`HumanBone::REQUIRED`] roles no bone plays.
    pub fn missing(&self) -> Vec<HumanBone> {
        HumanBone::REQUIRED
            .into_iter()
            .filter(|&role| self.get(role).is_none())
            .collect()
    }

    /// How many of `side`'s finger roles have a bone.
    pub fn finger_count(&self, side: Side) -> usize {
        HumanBone::fingers(side)
            .filter(|&role| self.get(role).is_some())
            .count()
    }

    /// As a rig definition naming every role's bone in `bones`.
    pub fn to_definition(&self, bones: &[Bone]) -> RigDefinition {
        let mut definition = RigDefinition::new();
        for (role, bone) in self.roles() {
            if let Some(bone) = bones.get(bone) {
                definition
                    .bones
                    .insert(role.name().to_owned(), bone.name.clone());
            }
        }
        definition
    }
}

/// A bone name's kind and side (`None` for the middle of the body), if
/// it's a humanoid bone's name. Namespaces (`mixamorig:`), common prefixes
/// (`DEF-`, `Bip01`) and side markers (`Left`, `_l`, `.L`, ` L `, DAZ's
/// `lHand`) are taken off first; end bones, twist bones and metacarpals
/// aren't humanoid bones.
fn classify(name: &str) -> Option<(Kind, Option<Side>)> {
    let lower = name.to_ascii_lowercase();
    let lower = lower.rsplit([':', '|']).next().unwrap_or(&lower);
    let mut side = None;
    let mut words: Vec<String> = Vec::new();
    for word in lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
    {
        match word {
            "l" | "left" => side = Some(Side::Left),
            "r" | "right" => side = Some(Side::Right),
            "def" | "org" | "mch" | "bip01" | "bip001" | "bip" | "j" | "mixamorig" | "armature"
            | "rig" | "cc" | "base" | "jnt" | "bn" | "b" => {}
            _ => words.push(word.to_owned()),
        }
    }
    let mut key = words.concat();
    for (prefix, found) in [("left", Side::Left), ("right", Side::Right)] {
        if let Some(rest) = key.strip_prefix(prefix) {
            side = Some(found);
            key = rest.to_owned();
        }
    }
    if let Some(kind) = kind_of(&key) {
        return Some((kind, side));
    }
    // DAZ glues the side on: `lForearmBend`, `rThumb2`.
    if side.is_none() {
        for (prefix, found) in [('l', Side::Left), ('r', Side::Right)] {
            let kind = key.strip_prefix(prefix).and_then(kind_of);
            // Only limbs and fingers have sides (`lHip` isn't the hips).
            if let Some(kind) = kind
                && !matches!(kind, Kind::Hips | Kind::Spine | Kind::Neck | Kind::Head)
            {
                return Some((kind, Some(found)));
            }
        }
    }
    None
}

/// The kind of bone `key` (a name lower-cased, its side and prefixes taken
/// off, its words run together) is.
fn kind_of(key: &str) -> Option<Kind> {
    if let Some(finger) = finger_of(key) {
        return Some(finger);
    }
    // Spine1, spine_01, Spine.003, Toe0: the number only orders them.
    let base = key.trim_end_matches(|c: char| c.is_ascii_digit());
    Some(match base {
        "hips" | "hip" | "pelvis" => Kind::Hips,
        "spine" | "chest" | "upperchest" | "torso" | "abdomen" | "abdomenupper"
        | "abdomenlower" | "waist" => Kind::Spine,
        "neck" | "necklower" => Kind::Neck,
        "head" => Kind::Head,
        "shoulder" | "clavicle" | "collar" | "collarbone" => Kind::Shoulder,
        "upperarm" | "arm" | "uparm" | "shldr" | "shldrbend" => Kind::UpperArm,
        "forearm" | "lowerarm" | "elbow" | "forearmbend" => Kind::LowerArm,
        "hand" | "wrist" => Kind::Hand,
        "upleg" | "thigh" | "upperleg" | "thighbend" => Kind::UpperLeg,
        "leg" | "calf" | "shin" | "lowerleg" | "knee" => Kind::LowerLeg,
        "foot" | "ankle" => Kind::Foot,
        "toe" | "toes" | "toebase" | "ball" => Kind::Toes,
        _ => return None,
    })
}

/// A finger bone's digit and segment, from the names tools give them:
/// Mixamo's `HandIndex1`, Unreal's `index_01`, Blender's `f_index.01` and
/// `thumb.01`, Unity's `IndexProximal`, DAZ's `Index1` and `Mid1`, 3ds
/// Max Biped's `Finger1` (index), `Finger12` (its third bone). Numbers
/// past three (end bones) and metacarpals aren't finger bones.
fn finger_of(key: &str) -> Option<Kind> {
    let key = key.strip_prefix("hand").unwrap_or(key);
    // Biped: `finger` and the finger's digit (0 the thumb), then the bone's
    // (none, 1 or 2).
    if let Some(rest) = key.strip_prefix("finger") {
        let mut digits = rest.chars();
        let digit = match digits.next()? {
            '0' => Digit::Thumb,
            '1' => Digit::Index,
            '2' => Digit::Middle,
            '3' => Digit::Ring,
            '4' => Digit::Little,
            _ => return None,
        };
        let segment = match digits.as_str() {
            "" => Segment::Proximal,
            "1" => Segment::Intermediate,
            "2" => Segment::Distal,
            _ => return None,
        };
        return Some(Kind::Finger(digit, segment));
    }
    let names: [(&str, Digit); 9] = [
        ("thumb", Digit::Thumb),
        ("index", Digit::Index),
        ("middle", Digit::Middle),
        ("mid", Digit::Middle),
        ("ring", Digit::Ring),
        ("pinky", Digit::Little),
        ("little", Digit::Little),
        ("small", Digit::Little),
        ("pinkie", Digit::Little),
    ];
    // Blender names fingers other than the thumb `f_index`.
    let key = match key.strip_prefix('f') {
        Some(rest) if names.iter().any(|(name, _)| rest.starts_with(name)) => rest,
        _ => key,
    };
    let (rest, digit) = names
        .iter()
        // `pinkie` before `pinky`'s prefix doesn't matter; `middle` must be
        // tried before `mid`.
        .find_map(|&(name, digit)| Some((key.strip_prefix(name)?, digit)))?;
    let segment = match rest {
        "proximal" => Segment::Proximal,
        "intermediate" => Segment::Intermediate,
        "distal" => Segment::Distal,
        number if !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) => {
            match number.parse::<u32>().ok()? {
                1 => Segment::Proximal,
                2 => Segment::Intermediate,
                3 => Segment::Distal,
                _ => return None,
            }
        }
        _ => return None,
    };
    Some(Kind::Finger(digit, segment))
}
