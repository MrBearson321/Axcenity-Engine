//! The inspector's Bevel: a raised or sunken edge, written as a pair of
//! inset `box-shadow`s into the element's own `#Id` rule, a light one on
//! the side the light comes from and a dark one opposite. On a button the
//! flipped pair goes into `#Id:active`, so pressing it looks pushed in.
//! Editing the bevel rewrites only that pair: the element's other shadows
//! stay as they are.
//!
//! The pair is found again by its colours (pure white and pure black, both
//! inset, moved opposite ways by the same blur). Which comes first says
//! which it is: white first for Raised, black first for Sunken.

use axcenity_core::{ClassName, InstanceId, Value};
use axcenity_ui::{BoxShadow, UiState};
use eframe::egui;

use super::states::state_selector;
use super::text_toolbar::number;
use super::{DesignerAction, DesignerScene};
use crate::responsive;
use crate::widgets;

/// The light edge's colour.
const LIGHT: &str = "rgba(255, 255, 255, 0.6)";
/// The shaded edge's colour.
const DARK: &str = "rgba(0, 0, 0, 0.45)";

/// A bevel, as the inspector edits it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bevel {
    /// Raised (lit where the light comes from), else sunken.
    pub raised: bool,
    /// How far in the edges reach, in pixels.
    pub depth: f32,
    /// How soft the edges are (the shadows' blur), in pixels.
    pub softness: f32,
    /// Where the light comes from, in degrees: 0 from the right, 90 from
    /// the top, 135 from the top left.
    pub angle: f32,
}

impl Default for Bevel {
    fn default() -> Self {
        Bevel {
            raised: true,
            depth: 3.0,
            softness: 4.0,
            angle: 135.0,
        }
    }
}

impl Bevel {
    /// The same bevel the other way out, for a pressed button.
    pub fn flipped(self) -> Bevel {
        Bevel {
            raised: !self.raised,
            ..self
        }
    }

    /// Its two inset shadows, as CSS, in the order that says which it is.
    pub fn shadows(self) -> [String; 2] {
        let radians = self.angle.to_radians();
        // Towards the light, on screen (y down).
        let toward = [radians.cos(), -radians.sin()];
        let depth = self.depth.max(0.0);
        // An inset shadow lies along the edges it's moved away from, so
        // the lit edge's shadow moves away from the light.
        let lit = [-toward[0] * depth, -toward[1] * depth];
        let shadow = |[x, y]: [f32; 2], color: &str| {
            format!(
                "inset {}px {}px {}px {color}",
                number(round(x)),
                number(round(y)),
                number(round(self.softness.max(0.0)))
            )
        };
        let unlit = [-lit[0], -lit[1]];
        if self.raised {
            [shadow(lit, LIGHT), shadow(unlit, DARK)]
        } else {
            [shadow(lit, DARK), shadow(unlit, LIGHT)]
        }
    }
}

/// `value` to two decimals, so written values stay short.
fn round(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

/// The bevel in a `box-shadow` list, and the index of its first shadow.
pub fn find_bevel(value: &str) -> Option<(Bevel, usize)> {
    let shadows: Vec<Option<BoxShadow>> = axcenity_ui::split_css_list(value)
        .into_iter()
        .map(BoxShadow::parse)
        .collect();
    let white = |shadow: &BoxShadow| [shadow.color.r, shadow.color.g, shadow.color.b] == [1.0; 3];
    let black = |shadow: &BoxShadow| [shadow.color.r, shadow.color.g, shadow.color.b] == [0.0; 3];
    shadows.windows(2).enumerate().find_map(|(index, pair)| {
        let [Some(first), Some(second)] = pair else {
            return None;
        };
        let opposite = (first.offset[0] + second.offset[0]).abs() < 0.02
            && (first.offset[1] + second.offset[1]).abs() < 0.02;
        if !(first.inset && second.inset && opposite && first.blur == second.blur) {
            return None;
        }
        let raised = if white(first) && black(second) {
            true
        } else if black(first) && white(second) {
            false
        } else {
            return None;
        };
        // The first shadow is the one on the lit side, moved away from it.
        let [x, y] = first.offset;
        let depth = x.hypot(y);
        let angle = if depth > 0.0 {
            (y.atan2(-x).to_degrees() + 360.0) % 360.0
        } else {
            Bevel::default().angle
        };
        Some((
            Bevel {
                raised,
                depth: round(depth),
                softness: first.blur,
                angle: angle.round(),
            },
            index,
        ))
    })
}

/// `value` (a `box-shadow`, or `None` for none) with its bevel replaced
/// by `bevel`'s pair, or taken out for `None`. The other shadows stay, in
/// order; a new bevel goes after them. Nothing left is `none`.
pub fn with_bevel(value: Option<&str>, bevel: Option<Bevel>) -> String {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("none"));
    let mut shadows: Vec<String> = value
        .map(|value| {
            axcenity_ui::split_css_list(value)
                .into_iter()
                .map(|shadow| shadow.trim().to_owned())
                .filter(|shadow| !shadow.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let at = match value.and_then(find_bevel) {
        Some((_, index)) => {
            shadows.drain(index..index + 2);
            index
        }
        None => shadows.len(),
    };
    if let Some(bevel) = bevel {
        let [first, second] = bevel.shadows();
        shadows.splice(at..at, [first, second]);
    }
    if shadows.is_empty() {
        "none".to_owned()
    } else {
        shadows.join(", ")
    }
}

/// What a Bevel edit writes: `box-shadow` for `#Id`, and for `#Id:active`
/// when it's a button or that rule already has its own shadows.
pub fn bevel_values(
    scene: &DesignerScene,
    id: InstanceId,
    bevel: Option<Bevel>,
) -> (String, Option<String>) {
    let world = scene.world;
    let sheets = scene.stylesheets;
    let shadows = |state| axcenity_ui::style_value_in_state(world, id, sheets, state, "box-shadow");
    let normal = with_bevel(shadows(UiState::Normal).as_deref(), bevel);
    let button = world
        .get(id)
        .is_some_and(|instance| instance.class() == ClassName::TextButton);
    let pressed_rule = world
        .get(id)
        .and_then(|instance| instance.property("Id"))
        .and_then(Value::as_str)
        .filter(|own| !own.is_empty())
        .is_some_and(|own| {
            axcenity_ui::rule_declaration(
                sheets,
                &state_selector(own, UiState::Pressed),
                "box-shadow",
            )
            .is_some()
        });
    let pressed = ((button && bevel.is_some()) || pressed_rule).then(|| {
        with_bevel(
            shadows(UiState::Pressed).as_deref(),
            bevel.map(Bevel::flipped),
        )
    });
    (normal, pressed)
}

/// The Bevel section: None, Raised or Sunken, then its depth, softness
/// and light angle.
pub(super) fn bevel_editor(
    ui: &mut egui::Ui,
    scene: &DesignerScene,
    id: InstanceId,
    actions: &mut Vec<DesignerAction>,
) {
    let current = axcenity_ui::style_value_in_state(
        scene.world,
        id,
        scene.stylesheets,
        UiState::Normal,
        "box-shadow",
    )
    .and_then(|value| find_bevel(&value))
    .map(|(bevel, _)| bevel);
    ui.add_space(4.0);
    widgets::section_label(ui, "Bevel");
    let mut edited: Option<(Option<Bevel>, bool)> = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (label, choice) in [
            ("None", None),
            ("Raised", Some(true)),
            ("Sunken", Some(false)),
        ] {
            let on = current.map(|bevel| bevel.raised) == choice;
            let tip = match choice {
                None => "No bevel",
                Some(true) => "Lit on the side the light comes from, as if it stands out",
                Some(false) => "Shaded on the side the light comes from, as if it's pressed in",
            };
            if ui.selectable_label(on, label).on_hover_text(tip).clicked() && !on {
                let bevel = choice.map(|raised| Bevel {
                    raised,
                    ..current.unwrap_or_default()
                });
                edited = Some((bevel, false));
            }
        }
    });
    if let Some(mut bevel) = current {
        // Whether a value below was changed, and by dragging.
        let mut tuned = None;
        responsive::form(ui, ("ui designer bevel", id), |ui| {
            for (label, value, range, speed, suffix) in [
                ("Depth", &mut bevel.depth, 0.5..=24.0, 0.1, " px"),
                ("Softness", &mut bevel.softness, 0.0..=32.0, 0.1, " px"),
                ("Light", &mut bevel.angle, 0.0..=359.0, 1.0, "°"),
            ] {
                ui.label(label);
                let response = ui.add(
                    egui::DragValue::new(value)
                        .range(range)
                        .speed(speed)
                        .suffix(suffix),
                );
                ui.end_row();
                if response.changed() {
                    tuned = Some(response.dragged());
                }
            }
        });
        if let Some(dragging) = tuned {
            edited = Some((Some(bevel), dragging));
        }
        if scene
            .world
            .get(id)
            .is_some_and(|instance| instance.class() == ClassName::TextButton)
        {
            ui.add(
                egui::Label::new(
                    egui::RichText::new("Pressed shows it the other way out (#Id:active).").weak(),
                )
                .wrap(),
            );
        }
    }
    if let Some((bevel, dragging)) = edited {
        let (normal, pressed) = bevel_values(scene, id, bevel);
        actions.push(DesignerAction::Bevel {
            id,
            normal,
            pressed,
            dragging,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bevel_is_written_as_two_inset_shadows_and_read_back() {
        let raised = Bevel::default();
        let [light, dark] = raised.shadows();
        // Light from the top left: the light shadow moves down and right,
        // so it lies along the top and left edges.
        assert_eq!(light, "inset 2.12px 2.12px 4px rgba(255, 255, 255, 0.6)");
        assert_eq!(dark, "inset -2.12px -2.12px 4px rgba(0, 0, 0, 0.45)");
        let written = with_bevel(None, Some(raised));
        assert_eq!(find_bevel(&written), Some((raised, 0)));
        let sunken = raised.flipped();
        assert_eq!(
            find_bevel(&with_bevel(Some(&written), Some(sunken))).map(|(bevel, _)| bevel),
            Some(sunken)
        );
        let from_the_right = Bevel {
            angle: 0.0,
            depth: 5.0,
            ..raised
        };
        assert_eq!(
            find_bevel(&with_bevel(None, Some(from_the_right))).map(|(bevel, _)| bevel),
            Some(from_the_right)
        );
    }

    #[test]
    fn editing_the_bevel_keeps_the_other_shadows() {
        let drop = "0 4px 8px rgba(0, 0, 0, 0.3)";
        let with = with_bevel(Some(drop), Some(Bevel::default()));
        assert!(with.starts_with(&format!("{drop}, inset")), "{with}");
        let deeper = with_bevel(
            Some(&with),
            Some(Bevel {
                depth: 6.0,
                ..Bevel::default()
            }),
        );
        assert!(
            deeper.starts_with(&format!("{drop}, inset 4.24px")),
            "{deeper}"
        );
        assert_eq!(deeper.matches("inset").count(), 2);
        assert_eq!(with_bevel(Some(&deeper), None), drop);
        assert_eq!(with_bevel(Some("none"), None), "none");
        // Two inset shadows that aren't white and black aren't a bevel.
        assert_eq!(
            find_bevel("inset 2px 2px 4px #ff0000, inset -2px -2px 4px #00ff00"),
            None
        );
    }
}
