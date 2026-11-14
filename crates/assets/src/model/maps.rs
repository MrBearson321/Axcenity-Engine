//! What a texture is for (its role), guessed from its file name when the
//! model doesn't say, and how a role fills in a material's maps.
//!
//! Exporters and texturing tools name maps by convention: `Bench_BaseColor`,
//! `T_Bench_N`, `rock_ORM`, `Crate_MetallicSmoothness`. Some of these are
//! channel-packed, several grey maps in one image's channels; a [`Packing`]
//! says which channel holds which map.

use std::sync::Arc;

use super::{AlphaMode, Channel, ChannelTexture, Image, ModelMaterial};

/// What a texture is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    BaseColor,
    /// `flip_green` for DirectX-style maps (`_NormalDX`).
    Normal {
        flip_green: bool,
    },
    Metalness,
    Roughness,
    /// Roughness, inverted: white is smooth.
    Glossiness,
    Specular,
    Emissive,
    Opacity,
    Occlusion,
    /// A height (displacement or bump) map, which the engine can't use.
    Height,
    Packed(Packing),
}

/// Which channel of a channel-packed texture holds which map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Packing {
    /// Occlusion, roughness, metalness in red, green, blue (glTF, Unreal; also "ARM").
    Orm,
    /// Roughness, metalness, occlusion in red, green, blue.
    Rma,
    /// Metalness, roughness, occlusion in red, green, blue.
    Mra,
    /// glTF's metallic-roughness: roughness in green, metalness in blue.
    MetallicRoughness,
    /// Unity's: metalness in red, smoothness in alpha.
    MetallicSmoothness,
    /// Unity HDRP's mask map: metalness in red, occlusion in green
    /// (detail mask in blue, unused), smoothness in alpha.
    MaskMap,
    /// Unity's specular setup: specular color, smoothness in alpha.
    SpecularSmoothness,
    /// Base color with opacity in alpha (`_AlbedoTransparency`).
    ColorAlpha,
}

/// Words (lowercase, run together) and the role they name. Longer names
/// are matched first, so `basecolor` wins over `color`.
const WORDS: &[(&str, Role)] = &[
    ("occlusionroughnessmetallic", Role::Packed(Packing::Orm)),
    ("occlusionroughnessmetalness", Role::Packed(Packing::Orm)),
    ("aoroughnessmetallic", Role::Packed(Packing::Orm)),
    ("orm", Role::Packed(Packing::Orm)),
    ("arm", Role::Packed(Packing::Orm)),
    ("rma", Role::Packed(Packing::Rma)),
    ("mra", Role::Packed(Packing::Mra)),
    (
        "metallicroughness",
        Role::Packed(Packing::MetallicRoughness),
    ),
    ("metalroughness", Role::Packed(Packing::MetallicRoughness)),
    (
        "metallicsmoothness",
        Role::Packed(Packing::MetallicSmoothness),
    ),
    ("metalsmoothness", Role::Packed(Packing::MetallicSmoothness)),
    ("metallicgloss", Role::Packed(Packing::MetallicSmoothness)),
    ("maskmap", Role::Packed(Packing::MaskMap)),
    (
        "specularsmoothness",
        Role::Packed(Packing::SpecularSmoothness),
    ),
    (
        "specularglossiness",
        Role::Packed(Packing::SpecularSmoothness),
    ),
    ("specgloss", Role::Packed(Packing::SpecularSmoothness)),
    ("albedotransparency", Role::Packed(Packing::ColorAlpha)),
    ("albedoalpha", Role::Packed(Packing::ColorAlpha)),
    ("basecoloralpha", Role::Packed(Packing::ColorAlpha)),
    ("diffusealpha", Role::Packed(Packing::ColorAlpha)),
    ("albedo", Role::BaseColor),
    ("basecolor", Role::BaseColor),
    ("basecolour", Role::BaseColor),
    ("basemap", Role::BaseColor),
    ("diffuse", Role::BaseColor),
    ("diff", Role::BaseColor),
    ("color", Role::BaseColor),
    ("colour", Role::BaseColor),
    ("col", Role::BaseColor),
    ("alb", Role::BaseColor),
    ("normaldirectx", Role::Normal { flip_green: true }),
    ("normaldx", Role::Normal { flip_green: true }),
    ("nrmdx", Role::Normal { flip_green: true }),
    // Poly Haven's `nor_dx` / `nor_gl`, seen whole before `nor` alone.
    ("nordx", Role::Normal { flip_green: true }),
    ("norgl", Role::Normal { flip_green: false }),
    ("normalopengl", Role::Normal { flip_green: false }),
    ("normalgl", Role::Normal { flip_green: false }),
    ("normalmap", Role::Normal { flip_green: false }),
    ("normals", Role::Normal { flip_green: false }),
    ("normal", Role::Normal { flip_green: false }),
    ("norm", Role::Normal { flip_green: false }),
    ("nrm", Role::Normal { flip_green: false }),
    ("nor", Role::Normal { flip_green: false }),
    ("metallic", Role::Metalness),
    ("metalness", Role::Metalness),
    ("metal", Role::Metalness),
    ("roughness", Role::Roughness),
    ("rough", Role::Roughness),
    ("rgh", Role::Roughness),
    ("glossiness", Role::Glossiness),
    ("gloss", Role::Glossiness),
    ("smoothness", Role::Glossiness),
    ("smooth", Role::Glossiness),
    ("specular", Role::Specular),
    ("spec", Role::Specular),
    ("emissive", Role::Emissive),
    ("emission", Role::Emissive),
    ("emit", Role::Emissive),
    ("glow", Role::Emissive),
    ("selfillumination", Role::Emissive),
    ("illumination", Role::Emissive),
    ("opacity", Role::Opacity),
    ("transparency", Role::Opacity),
    ("transparent", Role::Opacity),
    ("alpha", Role::Opacity),
    ("cutout", Role::Opacity),
    ("ambientocclusion", Role::Occlusion),
    ("occlusion", Role::Occlusion),
    ("ao", Role::Occlusion),
    ("occ", Role::Occlusion),
    ("height", Role::Height),
    ("displacement", Role::Height),
    ("disp", Role::Height),
    ("bump", Role::Height),
];

/// Short suffixes (`T_Bench_N`), matched only as a file name's last word,
/// where they're a convention rather than part of another word.
const SUFFIXES: &[(&str, Role)] = &[
    ("d", Role::BaseColor),
    ("c", Role::BaseColor),
    ("bc", Role::BaseColor),
    ("a", Role::BaseColor),
    ("n", Role::Normal { flip_green: false }),
    ("nm", Role::Normal { flip_green: false }),
    ("m", Role::Metalness),
    ("r", Role::Roughness),
    ("g", Role::Glossiness),
    ("s", Role::Specular),
    ("e", Role::Emissive),
    ("o", Role::Opacity),
    ("h", Role::Height),
];

/// The role a texture's file name suggests, if any: `Bench_BaseColor_4K.png`
/// is a base color, `rock_ORM.png` packed occlusion, roughness and
/// metalness. The last telling word decides, since conventions put the
/// role at the end.
pub(crate) fn role_from_name(file_name: &str) -> Option<Role> {
    let stem = file_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .split('.')
        .next()
        .unwrap_or_default();
    let words = words(stem);
    // Sizes and numbers say nothing ("4k", "2048", "01").
    let words: Vec<&str> = words
        .iter()
        .map(String::as_str)
        .filter(|word| !word.chars().all(|c| c.is_ascii_digit()) && !is_size(word))
        .collect();
    if let Some(last) = words.last()
        && let Some(&(_, role)) = SUFFIXES.iter().find(|(suffix, _)| suffix == last)
    {
        // "T_Bench_N", but not a one-word name such as "n.png".
        if words.len() > 1 {
            return Some(role);
        }
    }
    for end in (1..=words.len()).rev() {
        // Words ending here, three, two and one long, so "Base" + "Color"
        // or "Occlusion" + "Roughness" + "Metallic" are seen whole.
        for length in (1..=3.min(end)).rev() {
            let joined: String = words[end - length..end].concat();
            if let Some(&(_, role)) = WORDS.iter().find(|(word, _)| *word == joined) {
                return Some(role);
            }
        }
    }
    None
}

/// Words that name a material as a material, not what it's of.
const MATERIAL_WORDS: &[&str] = &["mat", "material", "mtl", "shader", "sg", "m", "mi"];

/// A material's or texture's name as one lowercase run of letters and
/// digits, leaving out words such as "material" and "shader", for telling
/// which material a texture is for (`Bench_shader` is "bench").
pub(crate) fn name_key(name: &str) -> String {
    let stem = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let stem = stem.rsplit_once('.').map_or(stem, |(stem, _)| stem);
    words(stem)
        .into_iter()
        .filter(|word| {
            let letters = word.trim_end_matches(|c: char| c.is_ascii_digit());
            !MATERIAL_WORDS.contains(&letters)
        })
        .collect()
}

/// `stem` split into lowercase words at anything that isn't a letter or
/// digit, and where a lowercase letter is followed by a capital
/// (`BaseColor` is "base", "color").
fn words(stem: &str) -> Vec<String> {
    word_spans(stem).into_iter().map(|span| span.word).collect()
}

/// One of [`words`], with where it ends in the name.
struct WordSpan {
    /// The byte after its last letter in the name.
    end: usize,
    word: String,
}

/// [`words`], each with where it ends.
fn word_spans(stem: &str) -> Vec<WordSpan> {
    let mut spans = Vec::new();
    let mut word = String::new();
    let mut end = 0;
    let mut last_lower = false;
    for (at, c) in stem.char_indices() {
        if !c.is_alphanumeric() {
            if !word.is_empty() {
                spans.push(WordSpan {
                    end,
                    word: std::mem::take(&mut word),
                });
            }
            last_lower = false;
            continue;
        }
        if c.is_uppercase() && last_lower && !word.is_empty() {
            spans.push(WordSpan {
                end,
                word: std::mem::take(&mut word),
            });
        }
        end = at + c.len_utf8();
        last_lower = c.is_lowercase() || c.is_ascii_digit();
        word.extend(c.to_lowercase());
    }
    if !word.is_empty() {
        spans.push(WordSpan { end, word });
    }
    spans
}

/// Image formats, which download names often carry (`Moss002_2K-JPG_Color`).
const FORMAT_WORDS: &[&str] = &[
    "jpg", "jpeg", "png", "tga", "bmp", "exr", "hdr", "tif", "tiff", "webp",
];

/// Takes the sizes and formats (`2k`, `2K` split as "2" "k", `JPG`) off the
/// end of `spans`, and plain numbers too with `numbers`.
fn trim_sizes(spans: &mut Vec<WordSpan>, numbers: bool) {
    let digits = |word: &str| !word.is_empty() && word.chars().all(|c| c.is_ascii_digit());
    loop {
        let Some(last) = spans.last() else {
            return;
        };
        let word = last.word.as_str();
        if word == "k" && spans.len() >= 2 && digits(&spans[spans.len() - 2].word) {
            spans.truncate(spans.len() - 2);
        } else if is_size(word) || FORMAT_WORDS.contains(&word) || (numbers && digits(word)) {
            spans.pop();
        } else {
            return;
        }
    }
}

/// The material a texture map's file is one map of, by its name: the name
/// with the map's word and the sizes and formats around it taken off, so
/// every map of a download names the same material
/// (`brown_mud_leaves_01_nor_gl_2k.jpg` and `brown_mud_leaves_01_diff_2k.jpg`
/// are `brown_mud_leaves_01`, `Moss002_2K-JPG_Color.jpg` is `Moss002`).
/// `None` unless the name ends with a word saying which map it is.
pub fn texture_pack_name(file_name: &str) -> Option<String> {
    let file = file_name.rsplit(['/', '\\']).next().unwrap_or_default();
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    let mut spans = word_spans(stem);
    // Sizes, formats and numbers after the map's word say nothing about it
    // ("_2k", "_01").
    trim_sizes(&mut spans, true);
    // The map's word ends the name: up to three words seen whole
    // ("ambient" + "occlusion", Poly Haven's "nor" + "gl"), or a short
    // suffix after at least one other word ("T_Bench_N").
    let mut cut = None;
    for length in (1..=3.min(spans.len())).rev() {
        let joined: String = spans[spans.len() - length..]
            .iter()
            .map(|span| span.word.as_str())
            .collect();
        if WORDS.iter().any(|(word, _)| *word == joined) {
            cut = Some(spans.len() - length);
            break;
        }
    }
    if cut.is_none()
        && spans.len() > 1
        && let Some(last) = spans.last()
        && SUFFIXES.iter().any(|(suffix, _)| *suffix == last.word)
    {
        cut = Some(spans.len() - 1);
    }
    let cut = cut?;
    // What's before it, without the sizes and formats at its end; a number
    // stays ("forest_leaves_04").
    spans.truncate(cut);
    trim_sizes(&mut spans, false);
    let end = spans.last()?.end;
    let name = stem[..end].trim_matches(|c: char| !c.is_alphanumeric());
    (!name.is_empty()).then(|| name.to_owned())
}

/// True for a texture size such as "4k" or "2k".
fn is_size(word: &str) -> bool {
    word.strip_suffix('k')
        .is_some_and(|number| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()))
}

impl ModelMaterial {
    /// Uses `image` for `role`, replacing what that map had. Returns false
    /// if the role is one the engine can't use (a height map).
    pub(crate) fn set_map(&mut self, role: Role, image: Arc<Image>) -> bool {
        let channel = |channel| ChannelTexture::new(image.clone(), channel);
        let inverted = |channel| ChannelTexture {
            invert: true,
            ..ChannelTexture::new(image.clone(), channel)
        };
        match role {
            Role::BaseColor => self.base_color_texture = Some(image.clone()),
            Role::Normal { flip_green } => {
                self.normal_texture = Some(image.clone());
                self.normal_flip_green = flip_green;
            }
            Role::Metalness => {
                self.metalness_texture = Some(channel(Channel::Red));
                self.metalness = 1.0;
            }
            Role::Roughness => {
                self.roughness_texture = Some(channel(Channel::Red));
                self.roughness = 1.0;
            }
            Role::Glossiness => {
                self.roughness_texture = Some(inverted(Channel::Red));
                self.roughness = 1.0;
            }
            Role::Specular => self.specular_texture = Some(image.clone()),
            Role::Emissive => {
                self.emissive_texture = Some(image.clone());
                // With no emission color, it glows in the map's own colors.
                if self.emissive.iter().all(|&c| c <= 0.0) {
                    self.emissive = [1.0; 3];
                }
            }
            Role::Opacity => {
                self.opacity_texture = Some(channel(if image.has_alpha() {
                    Channel::Alpha
                } else {
                    Channel::Red
                }));
                self.cut_out();
            }
            Role::Occlusion => self.occlusion_texture = Some(channel(Channel::Red)),
            Role::Height => return false,
            Role::Packed(packing) => self.set_packed(packing, image),
        }
        true
    }

    fn set_packed(&mut self, packing: Packing, image: Arc<Image>) {
        let channel = |channel| ChannelTexture::new(image.clone(), channel);
        let smoothness = || ChannelTexture {
            invert: true,
            ..ChannelTexture::new(image.clone(), Channel::Alpha)
        };
        let (occlusion, roughness, metalness) = match packing {
            Packing::Orm => (
                Some(channel(Channel::Red)),
                channel(Channel::Green),
                Some(channel(Channel::Blue)),
            ),
            Packing::Rma => (
                Some(channel(Channel::Blue)),
                channel(Channel::Red),
                Some(channel(Channel::Green)),
            ),
            Packing::Mra => (
                Some(channel(Channel::Blue)),
                channel(Channel::Green),
                Some(channel(Channel::Red)),
            ),
            Packing::MetallicRoughness => {
                (None, channel(Channel::Green), Some(channel(Channel::Blue)))
            }
            Packing::MetallicSmoothness => (None, smoothness(), Some(channel(Channel::Red))),
            Packing::MaskMap => (
                Some(channel(Channel::Green)),
                smoothness(),
                Some(channel(Channel::Red)),
            ),
            Packing::SpecularSmoothness => {
                self.specular_texture = Some(image.clone());
                (None, smoothness(), None)
            }
            Packing::ColorAlpha => {
                self.base_color_texture = Some(image.clone());
                self.opacity_texture = Some(channel(Channel::Alpha));
                self.cut_out();
                return;
            }
        };
        if occlusion.is_some() {
            self.occlusion_texture = occlusion;
        }
        self.roughness_texture = Some(roughness);
        self.roughness = 1.0;
        if metalness.is_some() {
            self.metalness_texture = metalness;
            self.metalness = 1.0;
        }
    }

    /// The role `image` has in the material, if it's used: the first map
    /// it's in.
    pub(crate) fn role_of(&self, image: &Arc<Image>) -> Option<Role> {
        let is = |other: &Option<Arc<Image>>| other.as_ref().is_some_and(|o| Arc::ptr_eq(o, image));
        fn map_is<'a>(
            map: &'a Option<ChannelTexture>,
            image: &Arc<Image>,
        ) -> Option<&'a ChannelTexture> {
            map.as_ref().filter(|map| Arc::ptr_eq(&map.image, image))
        }
        if is(&self.base_color_texture) {
            Some(Role::BaseColor)
        } else if is(&self.normal_texture) {
            Some(Role::Normal {
                flip_green: self.normal_flip_green,
            })
        } else if map_is(&self.metalness_texture, image).is_some() {
            Some(Role::Metalness)
        } else if let Some(roughness) = map_is(&self.roughness_texture, image) {
            Some(if roughness.invert {
                Role::Glossiness
            } else {
                Role::Roughness
            })
        } else if map_is(&self.occlusion_texture, image).is_some() {
            Some(Role::Occlusion)
        } else if is(&self.emissive_texture) {
            Some(Role::Emissive)
        } else if map_is(&self.opacity_texture, image).is_some() {
            Some(Role::Opacity)
        } else if is(&self.specular_texture) {
            Some(Role::Specular)
        } else {
            None
        }
    }

    /// True if the material already has the map `role` would fill.
    pub(crate) fn has(&self, role: Role) -> bool {
        match role {
            Role::BaseColor | Role::Packed(Packing::ColorAlpha) => {
                self.base_color_texture.is_some()
            }
            Role::Normal { .. } => self.normal_texture.is_some(),
            Role::Metalness => self.metalness_texture.is_some(),
            Role::Roughness | Role::Glossiness | Role::Packed(_) => {
                self.roughness_texture.is_some()
            }
            Role::Specular => self.specular_texture.is_some(),
            Role::Emissive => self.emissive_texture.is_some(),
            Role::Opacity => self.opacity_texture.is_some(),
            Role::Occlusion => self.occlusion_texture.is_some(),
            Role::Height => false,
        }
    }

    /// Makes a solid material cut out where its opacity is below half.
    fn cut_out(&mut self) {
        if self.alpha_mode == AlphaMode::Opaque {
            self.alpha_mode = AlphaMode::Mask { cutoff: 0.5 };
        }
    }

    /// Takes `image` off every map that uses it, before it's used for
    /// another role.
    pub(crate) fn remove_image(&mut self, image: &Arc<Image>) {
        let is = |other: &Option<Arc<Image>>| other.as_ref().is_some_and(|o| Arc::ptr_eq(o, image));
        let channel_is = |other: &Option<ChannelTexture>| {
            other.as_ref().is_some_and(|o| Arc::ptr_eq(&o.image, image))
        };
        if is(&self.base_color_texture) {
            self.base_color_texture = None;
        }
        if is(&self.normal_texture) {
            self.normal_texture = None;
        }
        if is(&self.emissive_texture) {
            self.emissive_texture = None;
        }
        if is(&self.specular_texture) {
            self.specular_texture = None;
        }
        for map in [
            &mut self.metalness_texture,
            &mut self.roughness_texture,
            &mut self.occlusion_texture,
            &mut self.opacity_texture,
        ] {
            if channel_is(map) {
                *map = None;
            }
        }
    }
}

/// A material map a texture file can fill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapSlot {
    BaseColor,
    Normal,
    Metalness,
    Roughness,
    Occlusion,
    Height,
    Emissive,
    Opacity,
    Specular,
}

/// One map a texture file fills: which, from which channel, and how.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureRole {
    pub slot: MapSlot,
    pub channel: Channel,
    /// Read as 1 minus the value (smoothness as roughness).
    pub invert: bool,
    /// A DirectX-style normal map, whose green points down.
    pub flip_green: bool,
}

/// What a texture file is for, judged by its name the same way as model
/// textures (`Rock_BaseColor_2K.png`, `T_Rock_N.png`, `rock_ORM.png`): the
/// maps it fills. Empty when the name doesn't say.
pub fn texture_roles(file_name: &str) -> Vec<TextureRole> {
    let role = |slot, channel, invert| TextureRole {
        slot,
        channel,
        invert,
        flip_green: false,
    };
    use Channel::{Alpha, Blue, Green, Red};
    match role_from_name(file_name) {
        None => Vec::new(),
        Some(Role::BaseColor) | Some(Role::Packed(Packing::ColorAlpha)) => {
            vec![role(MapSlot::BaseColor, Red, false)]
        }
        Some(Role::Normal { flip_green }) => vec![TextureRole {
            flip_green,
            ..role(MapSlot::Normal, Red, false)
        }],
        Some(Role::Metalness) => vec![role(MapSlot::Metalness, Red, false)],
        Some(Role::Roughness) => vec![role(MapSlot::Roughness, Red, false)],
        Some(Role::Glossiness) => vec![role(MapSlot::Roughness, Red, true)],
        Some(Role::Occlusion) => vec![role(MapSlot::Occlusion, Red, false)],
        Some(Role::Height) => vec![role(MapSlot::Height, Red, false)],
        Some(Role::Emissive) => vec![role(MapSlot::Emissive, Red, false)],
        Some(Role::Opacity) => vec![role(MapSlot::Opacity, Red, false)],
        Some(Role::Specular) => vec![role(MapSlot::Specular, Red, false)],
        Some(Role::Packed(packing)) => match packing {
            Packing::Orm => vec![
                role(MapSlot::Occlusion, Red, false),
                role(MapSlot::Roughness, Green, false),
                role(MapSlot::Metalness, Blue, false),
            ],
            Packing::Rma => vec![
                role(MapSlot::Roughness, Red, false),
                role(MapSlot::Metalness, Green, false),
                role(MapSlot::Occlusion, Blue, false),
            ],
            Packing::Mra => vec![
                role(MapSlot::Metalness, Red, false),
                role(MapSlot::Roughness, Green, false),
                role(MapSlot::Occlusion, Blue, false),
            ],
            Packing::MetallicRoughness => vec![
                role(MapSlot::Roughness, Green, false),
                role(MapSlot::Metalness, Blue, false),
            ],
            Packing::MetallicSmoothness => vec![
                role(MapSlot::Metalness, Red, false),
                role(MapSlot::Roughness, Alpha, true),
            ],
            Packing::MaskMap => vec![
                role(MapSlot::Metalness, Red, false),
                role(MapSlot::Occlusion, Green, false),
                role(MapSlot::Roughness, Alpha, true),
            ],
            Packing::SpecularSmoothness => vec![
                role(MapSlot::Specular, Red, false),
                role(MapSlot::Roughness, Alpha, true),
            ],
            Packing::ColorAlpha => vec![role(MapSlot::BaseColor, Red, false)],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_come_from_the_last_telling_word() {
        let cases = [
            ("Bench_BaseColor_4K.png", Role::BaseColor),
            ("T_Bench_N.tga", Role::Normal { flip_green: false }),
            ("bench_normal_dx.png", Role::Normal { flip_green: true }),
            ("BenchNormalDX.png", Role::Normal { flip_green: true }),
            ("rock_ORM.png", Role::Packed(Packing::Orm)),
            (
                "Rock_OcclusionRoughnessMetallic.png",
                Role::Packed(Packing::Orm),
            ),
            (
                "Crate_MetallicSmoothness.png",
                Role::Packed(Packing::MetallicSmoothness),
            ),
            (
                "Crate_AlbedoTransparency.png",
                Role::Packed(Packing::ColorAlpha),
            ),
            ("wall_MaskMap.png", Role::Packed(Packing::MaskMap)),
            ("wall_mra.png", Role::Packed(Packing::Mra)),
            ("Metal_Rough.jpg", Role::Roughness),
            ("plank_gloss.png", Role::Glossiness),
            ("lamp-emissive.png", Role::Emissive),
            ("leaves_opacity.png", Role::Opacity),
            ("leaves_AO_01.png", Role::Occlusion),
            ("C:/Users/artist/maps/brick_Height.png", Role::Height),
            ("door_spec.bmp", Role::Specular),
            ("diffuse.png", Role::BaseColor),
        ];
        for (name, role) in cases {
            assert_eq!(role_from_name(name), Some(role), "{name}");
        }
        for name in ["wood.png", "n.png", "texture_01.png", ""] {
            assert_eq!(role_from_name(name), None, "{name}");
        }
    }

    #[test]
    fn poly_haven_normal_maps_say_which_convention_they_are() {
        // Poly Haven names them `<asset>_nor_dx_<size>` and `<asset>_nor_gl_<size>`.
        assert_eq!(
            role_from_name("red_brick_03_nor_dx_2k.jpg"),
            Some(Role::Normal { flip_green: true })
        );
        assert_eq!(
            role_from_name("red_brick_03_nor_gl_2k.jpg"),
            Some(Role::Normal { flip_green: false })
        );
        assert_eq!(
            role_from_name("red_brick_03_diff_2k.jpg"),
            Some(Role::BaseColor)
        );
        assert_eq!(
            role_from_name("red_brick_03_rough_2k.jpg"),
            Some(Role::Roughness)
        );
        assert_eq!(
            role_from_name("red_brick_03_arm_2k.jpg"),
            Some(Role::Packed(Packing::Orm))
        );
        assert_eq!(
            role_from_name("red_brick_03_disp_2k.png"),
            Some(Role::Height)
        );
    }

    fn image(rgba: [u8; 4]) -> Arc<Image> {
        Arc::new(Image::new(1, 1, rgba.to_vec()))
    }

    #[test]
    fn packed_maps_fill_each_map_from_its_channel() {
        let mut material = ModelMaterial::default();
        let mask = image([255, 128, 0, 200]);
        assert!(material.set_map(Role::Packed(Packing::MaskMap), mask.clone()));
        let metal = material.metalness_texture.as_ref().unwrap();
        assert_eq!(metal.channel, Channel::Red);
        let occlusion = material.occlusion_texture.as_ref().unwrap();
        assert_eq!(occlusion.channel, Channel::Green);
        let roughness = material.roughness_texture.as_ref().unwrap();
        assert_eq!(
            (roughness.channel, roughness.invert),
            (Channel::Alpha, true)
        );
        assert_eq!((material.metalness, material.roughness), (1.0, 1.0));

        // Used for something else, it leaves every map it was in.
        material.remove_image(&mask);
        assert!(material.metalness_texture.is_none() && material.roughness_texture.is_none());
        assert!(material.occlusion_texture.is_none());

        let colors = image([255, 255, 255, 0]);
        material.set_map(Role::Packed(Packing::ColorAlpha), colors);
        assert_eq!(
            material.opacity_texture.as_ref().unwrap().channel,
            Channel::Alpha
        );
        assert_eq!(material.alpha_mode, AlphaMode::Mask { cutoff: 0.5 });
        assert!(!material.set_map(Role::Height, image([0; 4])));
    }

    #[test]
    fn texture_sets_are_sorted_into_maps_by_name() {
        let slots = |name: &str| -> Vec<(MapSlot, Channel, bool)> {
            texture_roles(name)
                .into_iter()
                .map(|role| (role.slot, role.channel, role.invert))
                .collect()
        };
        assert_eq!(
            slots("Rock_BaseColor_2K.png"),
            vec![(MapSlot::BaseColor, Channel::Red, false)]
        );
        assert!(texture_roles("T_Rock_NormalDX.png")[0].flip_green);
        assert_eq!(
            slots("rock_ORM.png"),
            vec![
                (MapSlot::Occlusion, Channel::Red, false),
                (MapSlot::Roughness, Channel::Green, false),
                (MapSlot::Metalness, Channel::Blue, false),
            ]
        );
        assert_eq!(
            slots("grass_height.png"),
            vec![(MapSlot::Height, Channel::Red, false)]
        );
        assert!(texture_roles("photo.png").is_empty());
    }
}
