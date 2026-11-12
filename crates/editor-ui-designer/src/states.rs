//! The inspector's states editor: Normal, Hover, Pressed and Disabled tabs
//! for a button or text box (or any element with its own state rules). A
//! value edited in a tab goes into the element's own rule for that state
//! (`#play`, `#play:hover`, `#play:active` or `#play:disabled`), which is
//! made if it isn't there, so no CSS is typed. The text toolbar above writes
//! into the same rule. The eye shows the element in that state on the
//! artboard. Below the tabs, Transition (`transition` in `#play`) makes the
//! changes between states animate, and Interactable turns the element off.

use axcenity_core::{ClassName, InstanceId, Value, World};
use axcenity_ui::{Stylesheet, UiState};
use eframe::egui;

use super::text_toolbar::{css_color, number};
use super::{DesignerAction, DesignerScene, UiDesigner, state_style_action};
use crate::edit::Edit;
use crate::responsive;
use crate::style_panel;
use crate::theme::icons;
use crate::widgets;

/// The timing functions Transition offers, as CSS names them.
pub const TIMINGS: [&str; 5] = ["ease", "linear", "ease-in", "ease-out", "ease-in-out"];

/// What a new Transition is: CSS's `all 0.15s ease`.
const DEFAULT_TRANSITION: (f32, &str) = (0.15, "ease");

/// Whether `id` gets the states editor: a button or text box, or an
/// element with a rule of its own for a state.
pub fn has_states(world: &World, id: InstanceId, stylesheets: &[Stylesheet]) -> bool {
    let Some(instance) = world.get(id) else {
        return false;
    };
    if matches!(instance.class(), ClassName::TextButton | ClassName::TextBox) {
        return true;
    }
    own_id(world, id).is_some_and(|own| {
        UiState::ALL[1..]
            .iter()
            .any(|&state| axcenity_ui::has_rule(stylesheets, &state_selector(&own, state)))
    })
}

/// The states besides Normal that `id` has a rule of its own for, such as
/// `#play:hover`.
pub fn styled_states(world: &World, id: InstanceId, stylesheets: &[Stylesheet]) -> Vec<UiState> {
    let Some(own) = own_id(world, id) else {
        return Vec::new();
    };
    UiState::ALL[1..]
        .iter()
        .copied()
        .filter(|&state| axcenity_ui::has_rule(stylesheets, &state_selector(&own, state)))
        .collect()
}

/// `id`'s `Id`, if it has one.
fn own_id(world: &World, id: InstanceId) -> Option<String> {
    world
        .get(id)?
        .property("Id")
        .and_then(Value::as_str)
        .filter(|own| !own.is_empty())
        .map(str::to_owned)
}

/// The selector of an element's own rule for `state`, such as `#play:hover`.
pub fn state_selector(element_id: &str, state: UiState) -> String {
    format!(
        "{}{}",
        style_panel::own_selector(element_id),
        state.pseudo_class()
    )
}

/// The first `scale(…)` in a CSS `transform`, or 1.
pub fn scale_of(transform: Option<&str>) -> f32 {
    transform
        .and_then(|transform| {
            let start = transform.find("scale(")? + "scale(".len();
            let end = start + transform[start..].find(')')?;
            transform[start..end].split(',').next()?.trim().parse().ok()
        })
        .filter(|scale: &f32| scale.is_finite())
        .unwrap_or(1.0)
}

/// A `transition` as the editor writes it (`all 0.15s ease`): its
/// duration in seconds and its timing function. `None` for `none` or no
/// time.
pub fn transition_of(value: Option<&str>) -> Option<(f32, String)> {
    let value = value?.trim();
    let mut duration = None;
    let mut timing = "ease".to_owned();
    for word in value.split(',').next()?.split_whitespace() {
        if let Some(ms) = word.strip_suffix("ms") {
            duration = duration.or(ms.parse::<f32>().ok().map(|ms| ms / 1000.0));
        } else if let Some(seconds) = word.strip_suffix('s')
            && let Ok(seconds) = seconds.parse::<f32>()
        {
            duration = duration.or(Some(seconds));
        } else if TIMINGS.contains(&word) || word.starts_with("cubic-bezier(") {
            timing = word.to_owned();
        }
    }
    duration
        .filter(|duration| *duration > 0.0)
        .map(|duration| (duration, timing))
}

/// The background `id` has in `state`: its CSS `background-color`, else
/// its `BackgroundColor3` and `BackgroundTransparency`.
fn background(
    world: &World,
    id: InstanceId,
    stylesheets: &[Stylesheet],
    state: UiState,
) -> [f32; 4] {
    if let Some((color, alpha)) =
        axcenity_ui::style_value_in_state(world, id, stylesheets, state, "background-color")
            .and_then(|value| axcenity_ui::parse_css_color(&value))
    {
        return [color.r, color.g, color.b, alpha];
    }
    let Some(instance) = world.get(id) else {
        return [0.0; 4];
    };
    let color = instance
        .property("BackgroundColor3")
        .and_then(Value::as_color3)
        .unwrap_or(axcenity_core::Color3::new(0.0, 0.0, 0.0));
    let transparency = instance
        .property("BackgroundTransparency")
        .and_then(Value::as_number)
        .unwrap_or(0.0) as f32;
    [
        color.r,
        color.g,
        color.b,
        1.0 - transparency.clamp(0.0, 1.0),
    ]
}

impl UiDesigner {
    /// The tabs, the selected state's values, Transition and Interactable.
    pub(super) fn states_editor(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        id: InstanceId,
        actions: &mut Vec<DesignerAction>,
    ) {
        let world = scene.world;
        let Some(instance) = world.get(id) else {
            return;
        };
        let sheets = scene.stylesheets;
        ui.horizontal_wrapped(|ui| {
            // The four tabs, which wrap onto a second line in a narrow column.
            ui.spacing_mut().item_spacing.x = 2.0;
            for state in UiState::ALL {
                ui.selectable_value(&mut self.state, state, state.name());
            }
            let (icon, tip) = if self.show_state {
                (
                    icons::EYE,
                    "Showing it in this state on the artboard. Click to show it as it is.",
                )
            } else {
                (icons::EYE_SLASH, "Show it in this state on the artboard")
            };
            if widgets::icon_button(ui, icon, tip, self.show_state).clicked() {
                self.show_state = !self.show_state;
            }
        });
        let state = self.state;
        let element_id =
            own_id(world, id).unwrap_or_else(|| style_panel::id_from_name(instance.name()));
        let selector = state_selector(&element_id, state);
        let said = match state {
            UiState::Normal => format!("How it looks most of the time ({selector})."),
            UiState::Hover => format!(
                "While the pointer is over it ({selector}). What isn't set here stays as in Normal."
            ),
            UiState::Pressed => {
                format!("While it's held down ({selector}). Hover still applies under it.")
            }
            UiState::Disabled => {
                format!("While Interactable is off ({selector}): no hover, clicks or Activated.")
            }
        };
        ui.add(egui::Label::new(egui::RichText::new(said).weak()).wrap());

        let mut set = |property: &'static str, value: String, dragging: bool| {
            actions.push(state_style_action(id, state, property, value, dragging));
        };
        responsive::form(ui, ("ui designer states", id), |ui| {
            ui.label("Background");
            let mut bytes = background(world, id, sheets, state)
                .map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
            if ui
                .color_edit_button_srgba_unmultiplied(&mut bytes)
                .on_hover_text("Background colour (CSS background-color)")
                .changed()
            {
                set(
                    "background-color",
                    css_color(bytes.map(|byte| f32::from(byte) / 255.0)),
                    false,
                );
            }
            ui.end_row();

            ui.label("Scale");
            let transform =
                axcenity_ui::style_value_in_state(world, id, sheets, state, "transform");
            let mut scale = scale_of(transform.as_deref());
            let response = ui
                .add(
                    egui::DragValue::new(&mut scale)
                        .range(0.1..=3.0)
                        .speed(0.005)
                        .suffix(" ×"),
                )
                .on_hover_text(
                    "Drawn bigger or smaller around its middle, without moving anything else \
                     (CSS transform: scale)",
                );
            if response.changed() {
                set(
                    "transform",
                    format!("scale({})", number(scale)),
                    response.dragged(),
                );
            }
            ui.end_row();

            ui.label("Opacity");
            let mut opacity =
                axcenity_ui::style_value_in_state(world, id, sheets, state, "opacity")
                    .and_then(|value| value.trim().parse::<f32>().ok())
                    .unwrap_or(1.0);
            let response = ui
                .add(
                    egui::DragValue::new(&mut opacity)
                        .range(0.0..=1.0)
                        .speed(0.01),
                )
                .on_hover_text("1 is solid, 0 is invisible (CSS opacity)");
            if response.changed() {
                set("opacity", number(opacity), response.dragged());
            }
            ui.end_row();
        });

        // Transition and Interactable belong to the element, not one state.
        let transition = transition_of(
            axcenity_ui::style_value_in_state(world, id, sheets, UiState::Normal, "transition")
                .as_deref(),
        );
        let mut animate = transition.is_some();
        let (mut seconds, mut timing) =
            transition.unwrap_or((DEFAULT_TRANSITION.0, DEFAULT_TRANSITION.1.to_owned()));
        let mut write = None;
        ui.horizontal_wrapped(|ui| {
            if ui
                .checkbox(&mut animate, "Transition")
                .on_hover_text(
                    "Ease between states instead of snapping: colours, scale, opacity, sizes and \
                     shadows (CSS transition)",
                )
                .changed()
            {
                write = Some(false);
            }
            let response = ui.add_enabled(
                animate,
                egui::DragValue::new(&mut seconds)
                    .range(0.01..=5.0)
                    .speed(0.005)
                    .suffix(" s"),
            );
            if response.changed() {
                write = Some(response.dragged());
            }
            if animate {
                egui::ComboBox::from_id_salt(("ui designer timing", id))
                    .width(90.0)
                    .selected_text(timing.as_str())
                    .show_ui(ui, |ui| {
                        for name in TIMINGS {
                            if ui.selectable_label(timing == name, name).clicked() {
                                timing = name.to_owned();
                                write = Some(false);
                            }
                        }
                    });
            }
        });
        if let Some(dragging) = write {
            let value = if animate {
                format!("all {}s {timing}", number(seconds))
            } else {
                "none".to_owned()
            };
            actions.push(state_style_action(
                id,
                UiState::Normal,
                "transition",
                value,
                dragging,
            ));
        }
        let mut interactable =
            instance.property("Interactable").and_then(Value::as_bool) != Some(false);
        if ui
            .checkbox(&mut interactable, "Interactable")
            .on_hover_text("Off: Disabled's look, no hover, no clicks and no Activated")
            .changed()
        {
            actions.push(DesignerAction::Edit(
                id,
                Edit::Property("Interactable", Value::Bool(interactable)),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_and_transitions_are_read_back_as_the_editor_writes_them() {
        assert_eq!(scale_of(Some("scale(0.95)")), 0.95);
        assert_eq!(scale_of(Some("translate(2px, 0) scale(1.1, 1.1)")), 1.1);
        assert_eq!(scale_of(None), 1.0);
        assert_eq!(
            transition_of(Some("all 0.15s ease")),
            Some((0.15, "ease".to_owned()))
        );
        assert_eq!(
            transition_of(Some("all 200ms linear")),
            Some((0.2, "linear".to_owned()))
        );
        assert_eq!(transition_of(Some("none")), None);
        assert_eq!(state_selector("play", UiState::Pressed), "#play:active");
    }
}
