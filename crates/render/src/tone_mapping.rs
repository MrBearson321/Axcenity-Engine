//! The tone mapping curves: how the scene's linear, high dynamic range light
//! becomes colors from 0 to 1 that a screen can show. The composite pass in
//! `post.wgsl` runs them on the GPU; [`ToneMapping::apply`] is the same
//! math on the CPU, so tests can check each curve and the shader against it.

/// A tone mapping curve (Lighting's `ToneMapping`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToneMapping {
    /// Each channel on its own: up to 0.6 passes unchanged, and brighter
    /// light rolls off toward 1. It shifts the hue of bright colors.
    #[default]
    Classic,
    /// Khronos PBR Neutral: a lit base color comes out close to its sRGB
    /// value, and highlights roll off without changing hue.
    Neutral,
    /// Stephen Hill's fit of the ACES curves: more contrast, and very bright
    /// colors wash out toward white.
    Filmic,
}

impl ToneMapping {
    /// The curve's number in `post.wgsl` (the composite's `flags.z`).
    pub(crate) fn index(self) -> f32 {
        match self {
            ToneMapping::Classic => 0.0,
            ToneMapping::Neutral => 1.0,
            ToneMapping::Filmic => 2.0,
        }
    }

    /// The color shown for linear light `color` (after exposure), each
    /// channel from 0 to 1.
    pub fn apply(self, color: [f32; 3]) -> [f32; 3] {
        let mapped = match self {
            ToneMapping::Classic => color.map(classic),
            ToneMapping::Neutral => neutral(color),
            ToneMapping::Filmic => filmic(color),
        };
        mapped.map(|c| c.clamp(0.0, 1.0))
    }
}

/// Where the classic curve starts to bend.
const KNEE: f32 = 0.6;

fn classic(c: f32) -> f32 {
    if c <= KNEE {
        c
    } else {
        KNEE + (1.0 - KNEE) * (1.0 - (-(c - KNEE) / (1.0 - KNEE)).exp())
    }
}

/// Where Neutral starts to compress highlights.
const NEUTRAL_START: f32 = 0.8 - 0.04;
/// How much Neutral pulls the brightest highlights toward white.
const NEUTRAL_DESATURATION: f32 = 0.15;

/// Khronos PBR Neutral. It takes off the 4% every non-metal reflects (less
/// in the darkest colors, which it leaves almost untouched), so a lit color
/// comes out as it was picked.
fn neutral(color: [f32; 3]) -> [f32; 3] {
    let lowest = color[0].min(color[1]).min(color[2]);
    let offset = if lowest < 0.08 {
        lowest - 6.25 * lowest * lowest
    } else {
        0.04
    };
    let color = color.map(|c| c - offset);
    let peak = color[0].max(color[1]).max(color[2]);
    if peak < NEUTRAL_START {
        return color;
    }
    let d = 1.0 - NEUTRAL_START;
    let new_peak = 1.0 - d * d / (peak + d - NEUTRAL_START);
    let blend = 1.0 - 1.0 / (NEUTRAL_DESATURATION * (peak - new_peak) + 1.0);
    color.map(|c| {
        let scaled = c * new_peak / peak;
        scaled + (new_peak - scaled) * blend
    })
}

/// Filmic scales light up by this first, so a mid-grey comes out about as
/// bright as with the other curves (the fit alone darkens it).
const FILMIC_EXPOSURE: f32 = 1.0 / 0.6;

/// sRGB to the ACES working space, with the look's slight desaturation.
const ACES_INPUT: [[f32; 3]; 3] = [
    [0.59719, 0.35458, 0.04823],
    [0.07600, 0.90834, 0.01566],
    [0.02840, 0.13383, 0.83777],
];
/// Back from the ACES working space to sRGB.
const ACES_OUTPUT: [[f32; 3]; 3] = [
    [1.60475, -0.53108, -0.07367],
    [-0.10208, 1.10813, -0.00605],
    [-0.00327, -0.07276, 1.07602],
];

fn times(matrix: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    matrix.map(|row| row[0] * v[0] + row[1] * v[1] + row[2] * v[2])
}

fn filmic(color: [f32; 3]) -> [f32; 3] {
    let v = times(&ACES_INPUT, color.map(|c| c * FILMIC_EXPOSURE));
    let fitted =
        v.map(|v| (v * (v + 0.0245786) - 0.000090537) / (v * (0.983729 * v + 0.432951) + 0.238081));
    times(&ACES_OUTPUT, fitted)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CURVES: [ToneMapping; 3] = [
        ToneMapping::Classic,
        ToneMapping::Neutral,
        ToneMapping::Filmic,
    ];

    fn grey(curve: ToneMapping, light: f32) -> f32 {
        curve.apply([light; 3])[0]
    }

    fn srgb_level(linear: f32) -> f32 {
        let c = if linear <= 0.0031308 {
            linear * 12.92
        } else {
            1.055 * linear.powf(1.0 / 2.4) - 0.055
        };
        c * 255.0
    }

    #[test]
    fn every_curve_maps_black_to_black_rises_steadily_and_stays_at_or_under_one() {
        for curve in CURVES {
            assert_eq!(curve.apply([0.0; 3]), [0.0; 3], "{curve:?}");
            let mut previous = 0.0;
            for step in 1..=20_000 {
                let light = step as f32 * 0.001;
                let shown = grey(curve, light);
                assert!(shown >= previous, "{curve:?} falls at {light}");
                assert!(shown <= 1.0, "{curve:?} passes 1 at {light}");
                previous = shown;
            }
            // Ordinary light still gets brighter as it rises, rather than
            // flattening out early.
            assert!(grey(curve, 0.1) < grey(curve, 0.3), "{curve:?}");
            assert!(grey(curve, 0.5) < grey(curve, 1.0), "{curve:?}");
            assert!(grey(curve, 1.0) < grey(curve, 2.0), "{curve:?}");
            // Bright colors never go past 1 in any channel either.
            for color in [[20.0, 1.0, 0.1], [0.0, 50.0, 3.0], [4.0, 4.0, 9.0]] {
                let shown = curve.apply(color);
                assert!(shown.iter().all(|c| (0.0..=1.0).contains(c)), "{curve:?}");
            }
        }
    }

    /// A mid-grey plastic under white light reflects its own color plus the
    /// 4% every non-metal reflects. Neutral takes that 4% back off.
    #[test]
    fn neutral_keeps_a_lit_mid_grey_within_two_levels() {
        for level in [64.0f32, 128.0, 192.0] {
            let srgb = level / 255.0;
            let linear = ((srgb + 0.055) / 1.055).powf(2.4);
            let shown = srgb_level(grey(ToneMapping::Neutral, linear + 0.04));
            assert!((shown - level).abs() <= 2.0, "{level} came out as {shown}");
        }
    }

    #[test]
    fn neutral_and_filmic_keep_greys_grey_and_classic_passes_dim_light_unchanged() {
        for light in [0.05, 0.3, 1.0, 5.0] {
            for curve in [ToneMapping::Neutral, ToneMapping::Filmic] {
                let [r, g, b] = curve.apply([light; 3]);
                assert!((r - g).abs() < 1e-4 && (g - b).abs() < 1e-4, "{curve:?}");
            }
        }
        assert_eq!(grey(ToneMapping::Classic, 0.4), 0.4);
    }

    #[test]
    fn neutral_rolls_off_bright_colors_without_shifting_their_hue() {
        // Classic flattens each channel on its own, so a bright orange turns
        // yellow; Neutral keeps red well above green.
        let orange = [4.0, 2.0, 0.2];
        let [r, g, _] = ToneMapping::Neutral.apply(orange);
        let [cr, cg, _] = ToneMapping::Classic.apply(orange);
        assert!(r - g > cr - cg, "neutral {r} {g}, classic {cr} {cg}");
    }
}
