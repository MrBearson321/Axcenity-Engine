//! The viewport's ray tracing badge, at the bottom right: whether hardware
//! ray tracing is on for this GPU, which ray traced effects are drawing and
//! how many items rays see, so it's never a guess
//! (`docs/rendering/ray-tracing.md`). Hidden while Hardware ray tracing is
//! off and not opened.

use axcenity_render::{
    EffectiveGraphics, RayTracedAmbientOcclusion, RayTracedGlobalIllumination, RayTracedShadows,
    RayTracingStats,
};
use eframe::egui;

/// What the badge says.
#[derive(Debug, PartialEq)]
pub(super) struct Badge {
    /// The short line on the badge.
    pub text: String,
    /// More, on hover.
    pub detail: String,
    /// True while ray tracing is wanted but not drawing (a restart or a GPU
    /// that can't), shown in the warning color.
    pub warning: bool,
}

/// The ray traced effects `effective` draws, shortly: "Shadows High".
pub(super) fn traced_effects(effective: &EffectiveGraphics) -> Vec<String> {
    let mut effects = Vec::new();
    if effective.ray_traced_shadows != RayTracedShadows::Off {
        effects.push(format!("Shadows {}", effective.ray_traced_shadows.label()));
    }
    if effective.ray_traced_ambient_occlusion != RayTracedAmbientOcclusion::Off {
        effects.push(format!(
            "AO {}",
            effective.ray_traced_ambient_occlusion.label()
        ));
    }
    if effective.ray_traced_global_illumination != RayTracedGlobalIllumination::Off {
        effects.push(format!(
            "GI {}",
            effective.ray_traced_global_illumination.label()
        ));
    }
    effects
}

/// The badge for `effective` (its `ray_queries`: the GPU was opened with
/// ray tracing), whether Hardware ray tracing is `wanted` at the next
/// start, whether this GPU `can` ray trace, and this frame's `stats`. None
/// while it's neither wanted nor on.
pub(super) fn badge(
    effective: &EffectiveGraphics,
    wanted: bool,
    can: bool,
    stats: &RayTracingStats,
) -> Option<Badge> {
    let settings = "Settings > Graphics > Ray tracing";
    if !effective.ray_queries {
        if !wanted {
            return None;
        }
        return Some(if can {
            Badge {
                text: "Ray tracing: restart to turn on".to_owned(),
                detail: "Hardware ray tracing is on in the settings, but the GPU was opened \
                         without it. Restart the editor to ray trace."
                    .to_owned(),
                warning: true,
            }
        } else {
            Badge {
                text: "Ray tracing: not on this GPU".to_owned(),
                detail: "This GPU or driver can't ray trace (it needs DirectX 12 ray tracing \
                         tier 1.1, Vulkan ray queries or Metal on macOS 15). Everything is \
                         drawn without it."
                    .to_owned(),
                warning: true,
            }
        });
    }
    let reach = effective.ray_tracing_distance.round();
    let effects = traced_effects(effective);
    if effects.is_empty() {
        return Some(Badge {
            text: "Ray tracing on: no effects".to_owned(),
            detail: format!(
                "The GPU is ray tracing ready, but no ray traced effect is turned on, so \
                 nothing looks different yet. Turn one on in {settings}. View Mode > Ray Hits \
                 shows what rays would hit.{}",
                if wanted {
                    ""
                } else {
                    " Hardware ray tracing turns off when the editor restarts."
                }
            ),
            warning: false,
        });
    }
    let seen = if stats.instances == 0 {
        "nothing in reach yet".to_owned()
    } else {
        format!(
            "{} item{} traced",
            stats.instances,
            if stats.instances == 1 { "" } else { "s" }
        )
    };
    let waiting = if stats.meshes_waiting > 0 {
        format!(
            " {} more mesh{} wait to be built.",
            stats.meshes_waiting,
            if stats.meshes_waiting == 1 { "" } else { "es" }
        )
    } else {
        String::new()
    };
    Some(Badge {
        text: format!("Ray tracing: {} · {seen}", effects.join(", ")),
        detail: format!(
            "Drawing with ray tracing up to {reach} studs from the camera ({} meshes, {} \
             triangles); past that the usual shadow maps and screen-space effects take \
             over.{waiting} Change it in {settings}; the Profiler times each ray traced pass.",
            stats.meshes, stats.triangles
        ),
        warning: false,
    })
}

/// Draws `badge` at the bottom right of `rect`.
pub(super) fn paint(ui: &egui::Ui, rect: egui::Rect, badge: &Badge) {
    let palette = crate::theme::palette(ui.ctx());
    egui::Area::new(egui::Id::new("ray tracing badge"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::RIGHT_BOTTOM)
        .fixed_pos(rect.right_bottom() + egui::vec2(-10.0, -10.0))
        .constrain_to(rect)
        .interactable(true)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style())
                .inner_margin(egui::Margin::symmetric(8, 3))
                .show(ui, |ui| {
                    let color = if badge.warning {
                        palette.warning
                    } else {
                        palette.text
                    };
                    ui.add(
                        egui::Label::new(egui::RichText::new(&badge.text).small().color(color))
                            .sense(egui::Sense::hover()),
                    )
                    .on_hover_text(&badge.detail);
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_render::{Capabilities, GraphicsSettings};

    fn effective(ray_queries: bool, shadows: RayTracedShadows) -> EffectiveGraphics {
        GraphicsSettings {
            ray_traced_shadows: Some(shadows),
            ..GraphicsSettings::default()
        }
        .effective(&Capabilities {
            ray_queries,
            ..Capabilities::default()
        })
    }

    #[test]
    fn nothing_shows_while_ray_tracing_is_neither_wanted_nor_on() {
        let off = effective(false, RayTracedShadows::Off);
        assert_eq!(badge(&off, false, true, &RayTracingStats::default()), None);
    }

    #[test]
    fn wanted_but_not_opened_says_why_in_the_warning_color() {
        let off = effective(false, RayTracedShadows::High);
        let restart = badge(&off, true, true, &RayTracingStats::default()).unwrap();
        assert!(
            restart.warning && restart.text.contains("restart"),
            "{restart:?}"
        );
        let cannot = badge(&off, true, false, &RayTracingStats::default()).unwrap();
        assert!(
            cannot.warning && cannot.text.contains("not on this GPU"),
            "{cannot:?}"
        );
    }

    #[test]
    fn on_without_effects_says_nothing_looks_different_yet() {
        let on = effective(true, RayTracedShadows::Off);
        let shown = badge(&on, true, true, &RayTracingStats::default()).unwrap();
        assert_eq!(shown.text, "Ray tracing on: no effects");
        assert!(!shown.warning);
    }

    #[test]
    fn on_with_effects_names_them_and_counts_what_rays_see() {
        let on = effective(true, RayTracedShadows::Medium);
        let stats = RayTracingStats {
            instances: 812,
            meshes: 40,
            triangles: 120_000,
            ..RayTracingStats::default()
        };
        let shown = badge(&on, true, true, &stats).unwrap();
        assert_eq!(shown.text, "Ray tracing: Shadows Medium · 812 items traced");
        let both = GraphicsSettings {
            ray_traced_shadows: Some(RayTracedShadows::High),
            ray_traced_ambient_occlusion: Some(RayTracedAmbientOcclusion::Low),
            ray_traced_global_illumination: Some(RayTracedGlobalIllumination::Medium),
            ..GraphicsSettings::default()
        }
        .effective(&Capabilities {
            ray_queries: true,
            ..Capabilities::default()
        });
        assert_eq!(
            traced_effects(&both),
            ["Shadows High", "AO Low", "GI Medium"]
        );
        let empty = badge(&on, true, true, &RayTracingStats::default()).unwrap();
        assert!(
            empty.text.ends_with("nothing in reach yet"),
            "{}",
            empty.text
        );
    }
}
