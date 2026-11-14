//! The viewport's ray tracing badge, at the bottom right: whether hardware
//! ray tracing is on for this GPU, which ray traced effects are drawing and
//! how many items rays see, so it's never a guess
//! (`docs/rendering/ray-tracing.md`). Hidden while Hardware ray tracing is
//! off and not opened.

use axcenity_render::{
    EffectiveGraphics, RayTracedAmbientOcclusion, RayTracedGlobalIllumination,
    RayTracedReflections, RayTracedShadows, RayTracingStats,
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
    /// Whether the badge has a Compare toggle, and if so whether the ray
    /// traced effects are hidden by it right now.
    pub compare: Option<bool>,
}

/// What was clicked on the badge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Click {
    Restart,
    /// Compare: show the view with the ray traced effects hidden, or back.
    Compare,
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
    if effective.ray_traced_reflections != RayTracedReflections::Off {
        effects.push(format!(
            "Reflections {}",
            effective.ray_traced_reflections.label()
        ));
    }
    effects
}

/// The badge for `effective` (its `ray_queries`: the GPU was opened with
/// ray tracing), whether Hardware ray tracing is `wanted` at the next
/// start, whether this GPU `can` ray trace, what `AXCENITY_RAY_TRACING`
/// `forced` (`adapters::ray_tracing_override`), whether Compare has the
/// effects `hidden`, and this frame's `stats`. None while it's neither
/// wanted nor on.
pub(super) fn badge(
    effective: &EffectiveGraphics,
    wanted: bool,
    can: bool,
    forced: Option<bool>,
    hidden: bool,
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
                compare: None,
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
                compare: None,
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
                compare: None,
            }
        });
    }
    if hidden {
        return Some(Badge {
            text: "Ray tracing: hidden to compare".to_owned(),
            detail: "The view is drawn without its ray traced effects, as it looks with \
                     them off: shadow maps and screen-space ambient occlusion only. Click \
                     Compare again to bring the rays back. Nothing in the settings changed."
                .to_owned(),
            warning: false,
            restart: false,
            compare: Some(true),
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
            compare: None,
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
    // The world's ShadowDistance caps every shadow, traced or not: say so
    // when it's what stops them, or a farther ray tracing distance seems
    // to change nothing.
    let capped = stats.shadow_reach > 0 && (stats.shadow_reach as f32) < reach;
    let (cap, cap_detail) = if capped {
        (
            format!(" · shadows end at {} studs", stats.shadow_reach),
            format!(
                " Shadows (traced or not) end at {} studs: the world's Lighting \
                 ShadowDistance, which caps them whatever the ray tracing distance. Raise \
                 ShadowDistance in Lighting's properties to shadow farther.",
                stats.shadow_reach
            ),
        )
    } else {
        (String::new(), String::new())
    };
    Some(Badge {
        text: format!("Ray tracing: {} · {seen}{cap}", effects.join(", ")),
        detail: format!(
            "Drawing with ray tracing up to {reach} studs from the camera ({} meshes, {} \
             triangles); past that the usual shadow maps and screen-space effects take \
             over.{cap_detail}{waiting} Change it in {settings}; the Profiler times each ray \
             traced pass.",
            stats.meshes, stats.triangles
        ),
        warning: false,
        restart: false,
        compare: Some(false),
    })
}

/// Draws `badge` at the bottom right of `rect`, and says what on it was
/// clicked.
pub(super) fn paint(ui: &egui::Ui, rect: egui::Rect, badge: &Badge) -> Option<Click> {
    let palette = crate::theme::palette(ui.ctx());
    let mut click = None;
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
                        if badge.restart
                            && ui
                                .small_button("Restart now")
                                .on_hover_text(&badge.detail)
                                .clicked()
                        {
                            click = Some(Click::Restart);
                        }
                        if let Some(hidden) = badge.compare
                            && ui
                                .add(egui::Button::selectable(
                                    hidden,
                                    egui::RichText::new("Compare").small(),
                                ))
                                .on_hover_text(
                                    "Shows the view without its ray traced effects while on, \
                                     to see what they add. The settings don't change.",
                                )
                                .clicked()
                        {
                            click = Some(Click::Compare);
                        }
                    });
                });
        });
    click
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
            badge(&off, false, true, None, false, &RayTracingStats::default()),
            None
        );
    }

    #[test]
    fn wanted_but_not_opened_says_why_in_the_warning_color() {
        let off = effective(false, RayTracedShadows::High);
        let restart = badge(&off, true, true, None, false, &RayTracingStats::default()).unwrap();
        assert!(
            restart.warning && restart.restart && restart.text.contains("restart"),
            "{restart:?}"
        );
        let cannot = badge(&off, true, false, None, false, &RayTracingStats::default()).unwrap();
        assert!(
            cannot.warning && !cannot.restart && cannot.text.contains("not on this GPU"),
            "{cannot:?}"
        );
        // An environment variable keeping it off is named, with Restart now,
        // which starts the editor without it.
        let forced = badge(
            &off,
            true,
            true,
            Some(false),
            false,
            &RayTracingStats::default(),
        )
        .unwrap();
        assert!(
            forced.warning && forced.restart && forced.text.contains("AXCENITY_RAY_TRACING=0"),
            "{forced:?}"
        );
    }

    #[test]
    fn on_without_effects_says_nothing_looks_different_yet() {
        let on = effective(true, RayTracedShadows::Off);
        let shown = badge(&on, true, true, None, false, &RayTracingStats::default()).unwrap();
        assert_eq!(shown.text, "Ray tracing on: no effects");
        assert!(!shown.warning);
    }

    #[test]
    fn a_shadow_distance_nearer_than_the_rays_is_named_on_the_badge() {
        let on = effective(true, RayTracedShadows::High);
        let reach = on.ray_tracing_distance;
        let stats = RayTracingStats {
            instances: 10,
            shadow_reach: 150,
            ..RayTracingStats::default()
        };
        assert!(reach > 150.0, "{reach}");
        let shown = badge(&on, true, true, None, false, &stats).unwrap();
        assert!(
            shown.text.ends_with(" · shadows end at 150 studs"),
            "{}",
            shown.text
        );
        assert!(shown.detail.contains("ShadowDistance"), "{}", shown.detail);
        let reaching = RayTracingStats {
            shadow_reach: reach as u32,
            ..stats
        };
        let shown = badge(&on, true, true, None, false, &reaching).unwrap();
        assert!(!shown.text.contains("shadows end"), "{}", shown.text);
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
        let shown = badge(&on, true, true, None, false, &stats).unwrap();
        assert_eq!(shown.text, "Ray tracing: Shadows Medium · 812 items traced");
        assert_eq!(
            shown.compare,
            Some(false),
            "drawing rays: Compare is offered"
        );
        let hidden = badge(&on, true, true, None, true, &stats).unwrap();
        assert_eq!(hidden.compare, Some(true));
        assert!(hidden.text.contains("hidden to compare"), "{hidden:?}");
        let both = GraphicsSettings {
            ray_traced_shadows: Some(RayTracedShadows::High),
            ray_traced_ambient_occlusion: Some(RayTracedAmbientOcclusion::Low),
            ray_traced_global_illumination: Some(RayTracedGlobalIllumination::Medium),
            ray_traced_reflections: Some(RayTracedReflections::High),
            ..GraphicsSettings::default()
        }
        .effective(&Capabilities {
            ray_queries: true,
            ..Capabilities::default()
        });
        assert_eq!(
            traced_effects(&both),
            ["Shadows High", "AO Low", "GI Medium", "Reflections High"]
        );
        let empty = badge(&on, true, true, None, false, &RayTracingStats::default()).unwrap();
        assert!(
            empty.text.ends_with("nothing in reach yet"),
            "{}",
            empty.text
        );
    }
}
