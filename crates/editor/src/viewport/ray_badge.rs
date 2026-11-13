//! The viewport's ray tracing badge, at the bottom right: whether hardware
//! ray tracing is on for this GPU, which ray traced effects are drawing and
//! how many items rays see, so it's never a guess
//! (`docs/rendering/ray-tracing.md`). Hidden while Hardware ray tracing is
//! off and not opened.

use axcenity_render::{
    EffectiveGraphics, RayTracedAmbientOcclusion, RayTracedShadows, RayTracingStats,
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
    /// True when a restart would start ray tracing: the badge has a
    /// Restart now button.
    pub restart: bool,
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
    effects
}

/// The badge for `effective` (its `ray_queries`: the GPU was opened with
/// ray tracing), whether Hardware ray tracing is `wanted` at the next
/// start, whether this GPU `can` ray trace, what `AXCENITY_RAY_TRACING`
/// `forced` (`adapters::ray_tracing_override`), and this frame's `stats`.
/// None while it's neither wanted nor on.
pub(super) fn badge(
    effective: &EffectiveGraphics,
    wanted: bool,
    can: bool,
    forced: Option<bool>,
    stats: &RayTracingStats,
) -> Option<Badge> {
    let settings = "Settings > Graphics > Ray tracing";
    if !effective.ray_queries {
        if !wanted {
            return None;
        }
        return Some(if can && forced == Some(false) {
            Badge {
                text: "Ray tracing: kept off by AXCENITY_RAY_TRACING=0".to_owned(),
                detail: "The setting is on, but the editor was started with the environment \
                         variable AXCENITY_RAY_TRACING set to 0, which opens the GPU without \
                         ray tracing whatever the setting. Restart now starts the editor \
                         without it. In PowerShell, Remove-Item Env:AXCENITY_RAY_TRACING (or \
                         a new window) stops it coming back."
                    .to_owned(),
                warning: true,
                restart: true,
            }
        } else if can {
            Badge {
                text: "Ray tracing: restart to turn on".to_owned(),
                detail: "Hardware ray tracing is on in the settings, but the GPU was opened \
                         without it. Restart now starts the editor again with this project, \
                         keeping unsaved world and script edits. If it still says this \
                         afterward, check your Desktop shortcut opens this version of the \
                         editor."
                    .to_owned(),
                warning: true,
                restart: true,
            }
        } else {
            Badge {
                text: "Ray tracing: not on this GPU".to_owned(),
                detail: "This GPU or driver can't ray trace (it needs DirectX 12 ray tracing \
                         tier 1.1, Vulkan ray queries or Metal on macOS 15). Everything is \
                         drawn without it."
                    .to_owned(),
                warning: true,
                restart: false,
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
            restart: false,
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
        restart: false,
    })
}

/// Draws `badge` at the bottom right of `rect`. True when its Restart now
/// was clicked.
pub(super) fn paint(ui: &egui::Ui, rect: egui::Rect, badge: &Badge) -> bool {
    let palette = crate::theme::palette(ui.ctx());
    let mut restart = false;
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
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(&badge.text).small().color(color))
                                .sense(egui::Sense::hover()),
                        )
                        .on_hover_text(&badge.detail);
                        if badge.restart {
                            restart = ui
                                .small_button("Restart now")
                                .on_hover_text(&badge.detail)
                                .clicked();
                        }
                    });
                });
        });
    restart
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
        assert_eq!(
            badge(&off, false, true, None, &RayTracingStats::default()),
            None
        );
    }

    #[test]
    fn wanted_but_not_opened_says_why_in_the_warning_color() {
        let off = effective(false, RayTracedShadows::High);
        let restart = badge(&off, true, true, None, &RayTracingStats::default()).unwrap();
        assert!(
            restart.warning && restart.restart && restart.text.contains("restart"),
            "{restart:?}"
        );
        let cannot = badge(&off, true, false, None, &RayTracingStats::default()).unwrap();
        assert!(
            cannot.warning && !cannot.restart && cannot.text.contains("not on this GPU"),
            "{cannot:?}"
        );
        // An environment variable keeping it off is named, with Restart now,
        // which starts the editor without it.
        let forced = badge(&off, true, true, Some(false), &RayTracingStats::default()).unwrap();
        assert!(
            forced.warning && forced.restart && forced.text.contains("AXCENITY_RAY_TRACING=0"),
            "{forced:?}"
        );
    }

    #[test]
    fn on_without_effects_says_nothing_looks_different_yet() {
        let on = effective(true, RayTracedShadows::Off);
        let shown = badge(&on, true, true, None, &RayTracingStats::default()).unwrap();
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
        let shown = badge(&on, true, true, None, &stats).unwrap();
        assert_eq!(shown.text, "Ray tracing: Shadows Medium · 812 items traced");
        let both = GraphicsSettings {
            ray_traced_shadows: Some(RayTracedShadows::High),
            ray_traced_ambient_occlusion: Some(RayTracedAmbientOcclusion::Low),
            ..GraphicsSettings::default()
        }
        .effective(&Capabilities {
            ray_queries: true,
            ..Capabilities::default()
        });
        assert_eq!(traced_effects(&both), ["Shadows High", "AO Low"]);
        let empty = badge(&on, true, true, None, &RayTracingStats::default()).unwrap();
        assert!(
            empty.text.ends_with("nothing in reach yet"),
            "{}",
            empty.text
        );
    }
}
