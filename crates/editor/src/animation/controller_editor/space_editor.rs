//! A state's 2D blend space (Phase 78) in the inspector: its two Number
//! parameters, its clips at their points, each one's share in the preview
//! now, and a plot of the points with the preview's value among them.

use super::*;
use eframe::egui::{Color32, Pos2, Stroke, pos2, vec2};

/// The eight directions and the middle, as `[sideways, forward]`, with the
/// words a clip's name has for each (the first that fits wins).
pub(super) const DIRECTIONS: [([f64; 2], &[&str]); 9] = [
    ([0.0, 0.0], &["idle"]),
    ([0.0, 1.0], &["forward", "fwd", "walk"]),
    (
        [1.0, 1.0],
        &["forwardright", "forward_right", "fwdright", "fr"],
    ),
    ([1.0, 0.0], &["right"]),
    (
        [1.0, -1.0],
        &["backwardright", "backright", "back_right", "br"],
    ),
    ([0.0, -1.0], &["backward", "back"]),
    (
        [-1.0, -1.0],
        &["backwardleft", "backleft", "back_left", "bl"],
    ),
    ([-1.0, 0.0], &["left"]),
    (
        [-1.0, 1.0],
        &["forwardleft", "forward_left", "fwdleft", "fl"],
    ),
];

/// Which of [`DIRECTIONS`] a clip is for: the one with the longest word
/// its name (without spaces, dashes or underscores) ends with, so "Back
/// Left" is back-left and "Strafe Left" is left.
fn direction_of(clip: &str) -> Option<usize> {
    let name: String = axcenity_anim::files::clip_name(clip)
        .trim_end_matches(".anim.json")
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '_'))
        .flat_map(char::to_lowercase)
        .collect();
    DIRECTIONS
        .iter()
        .enumerate()
        .flat_map(|(index, (_, words))| words.iter().map(move |word| (index, *word)))
        .filter(|(_, word)| name.ends_with(&word.replace('_', "")))
        .max_by_key(|(_, word)| word.len())
        .map(|(index, _)| index)
}

/// The clip in `clips` for direction `direction` (an index into
/// [`DIRECTIONS`]), preferring one in `folder`.
fn direction_clip(clips: &[String], direction: usize, folder: Option<&str>) -> Option<String> {
    let fits = |clip: &&String| direction_of(clip) == Some(direction);
    let in_folder =
        |clip: &&String| folder.is_none_or(|f| super::super::store::group_of(clip) == f);
    clips
        .iter()
        .filter(fits)
        .find(in_folder)
        .or_else(|| clips.iter().find(fits))
        .cloned()
}

impl ControllerDoc {
    /// Turns state `index` into a blend space by the first two Number
    /// parameters (adding `MoveX` and `MoveZ` where there aren't two), its
    /// clip (if any) in the middle.
    pub(super) fn make_blend_space(&mut self, index: usize) {
        let mut numbers: Vec<String> = self
            .file
            .parameters
            .iter()
            .filter(|def| matches!(def, ParameterDef::Number { .. }))
            .map(|def| def.name().to_owned())
            .collect();
        for wanted in ["MoveX", "MoveZ"] {
            if numbers.len() >= 2 {
                break;
            }
            if numbers.iter().any(|name| name == wanted) {
                continue;
            }
            let name = self.free_parameter_name(wanted);
            self.file.parameters.push(ParameterDef::Number {
                name: name.clone(),
                default: 0.0,
            });
            numbers.push(name);
        }
        let children = match &self.file.states[index].motion {
            Motion::Clip(clip) if !clip.is_empty() => vec![BlendSpaceChild {
                clip: clip.clone(),
                position: [0.0, 0.0],
            }],
            _ => Vec::new(),
        };
        self.file.states[index].motion = Motion::BlendSpace(BlendSpace {
            parameter_x: numbers[0].clone(),
            parameter_y: numbers[1].clone(),
            children,
        });
    }

    /// Puts the eight directions and the middle in state `index`'s blend
    /// space, each with the project's clip of that name if there is one,
    /// keeping clips already at those points.
    pub(super) fn add_eight_directions(&mut self, index: usize, clips: &[String]) {
        let folder = self.file.states[index]
            .motion
            .clips()
            .into_iter()
            .find(|clip| !clip.is_empty())
            .map(|clip| super::super::store::group_of(clip).to_owned());
        let Motion::BlendSpace(space) = &mut self.file.states[index].motion else {
            return;
        };
        for (direction, (position, _)) in DIRECTIONS.iter().enumerate() {
            if space
                .children
                .iter()
                .any(|child| child.position == *position)
            {
                continue;
            }
            space.children.push(BlendSpaceChild {
                clip: direction_clip(clips, direction, folder.as_deref()).unwrap_or_default(),
                position: *position,
            });
        }
    }

    pub(super) fn blend_space_editor(&mut self, ui: &mut egui::Ui, index: usize, clips: &[String]) {
        let state_name = self.file.states[index].name.clone();
        let Motion::BlendSpace(space) = &self.file.states[index].motion else {
            return;
        };
        // The preview's mix now, and the parameters' values.
        let (weights, value) = match &self.preview {
            Some(run) => {
                let state = run.compiled.state_index(&state_name);
                let weights = state.and_then(|s| run.compiled.tree_weights(s, &run.parameters));
                let number = |name: &str| match run.parameters.get(name) {
                    Some(Parameter::Number(value)) => Some(*value),
                    _ => None,
                };
                let value = number(&space.parameter_x).zip(number(&space.parameter_y));
                (weights, value.map(|(x, y)| [x, y]))
            }
            None => (None, None),
        };
        let numbers: Vec<String> = self
            .file
            .parameters
            .iter()
            .filter(|def| matches!(def, ParameterDef::Number { .. }))
            .map(|def| def.name().to_owned())
            .collect();
        let mut directions = false;
        let Motion::BlendSpace(space) = &mut self.file.states[index].motion else {
            return;
        };
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.strong(format!("{} Blend space", icons::GRID_FOUR));
            for (label, parameter, id) in [
                ("Across (X)", &mut space.parameter_x, "space x"),
                ("Up (Y)", &mut space.parameter_y, "space y"),
            ] {
                ui.horizontal_wrapped(|ui| {
                    ui.label(label);
                    egui::ComboBox::from_id_salt(id)
                        .selected_text(parameter.as_str())
                        .show_ui(ui, |ui| {
                            for name in &numbers {
                                ui.selectable_value(parameter, name.clone(), name);
                            }
                        });
                });
            }
            ui.weak("Two Number parameters. At a clip's point it plays alone; between points, the three around the value share it.");
            let mut remove = None;
            egui::Grid::new("space children")
                .num_columns(5)
                .spacing([6.0, 4.0])
                .show(ui, |ui| {
                    ui.weak("Clip");
                    ui.weak("X");
                    ui.weak("Y");
                    ui.weak("Share");
                    ui.end_row();
                    for (c, child) in space.children.iter_mut().enumerate() {
                        clip_picker(ui, ("space clip", c), &mut child.clip, clips);
                        ui.add(egui::DragValue::new(&mut child.position[0]).speed(0.05));
                        ui.add(egui::DragValue::new(&mut child.position[1]).speed(0.05));
                        let share = weights.as_ref().and_then(|w| w.get(c)).copied().unwrap_or(0.0);
                        ui.add(
                            egui::ProgressBar::new(share as f32)
                                .desired_width(60.0)
                                .text(format!("{:.0}%", share * 100.0)),
                        );
                        if ui.small_button(icons::X).on_hover_text("Remove").clicked() {
                            remove = Some(c);
                        }
                        ui.end_row();
                    }
                });
            if let Some(c) = remove {
                space.children.remove(c);
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button(format!("{} Add clip", icons::PLUS)).clicked() {
                    space.children.push(BlendSpaceChild {
                        clip: String::new(),
                        position: [0.0, 0.0],
                    });
                }
                if ui
                    .button("Add 8 directions")
                    .on_hover_text("Idle in the middle and a clip each way at ±1, with the project's clips named Forward, Back, Left, Right and so on")
                    .clicked()
                {
                    directions = true;
                }
            });
            plot(ui, space, weights.as_deref(), value);
        });
        if directions {
            self.add_eight_directions(index, clips);
        }
    }
}

/// The blend space's points, each as large as its share, and the value.
fn plot(ui: &mut egui::Ui, space: &BlendSpace, weights: Option<&[f64]>, value: Option<[f64; 2]>) {
    let points: Vec<[f64; 2]> = space.children.iter().map(|c| c.position).collect();
    let mut low = [-1.0_f64, -1.0];
    let mut high = [1.0_f64, 1.0];
    for p in points.iter().chain(value.as_ref()) {
        for axis in 0..2 {
            low[axis] = low[axis].min(p[axis]);
            high[axis] = high[axis].max(p[axis]);
        }
    }
    let size = ui.available_width().clamp(120.0, 220.0);
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), egui::Sense::hover());
    let inner = rect.shrink(12.0);
    let at = |p: [f64; 2]| {
        let x = (p[0] - low[0]) / (high[0] - low[0]).max(1e-9);
        let y = (p[1] - low[1]) / (high[1] - low[1]).max(1e-9);
        pos2(
            inner.left() + x as f32 * inner.width(),
            inner.bottom() - y as f32 * inner.height(),
        )
    };
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.rect_filled(rect, 4.0, visuals.extreme_bg_color);
    let line = Stroke::new(1.0_f32, visuals.weak_text_color().gamma_multiply(0.4));
    for [a, b, c] in axcenity_anim::triangulate(&points) {
        painter.add(egui::Shape::closed_line(
            vec![at(points[a]), at(points[b]), at(points[c])],
            line,
        ));
    }
    let accent = visuals.selection.bg_fill;
    for (i, p) in points.iter().enumerate() {
        let share = weights.and_then(|w| w.get(i)).copied().unwrap_or(0.0) as f32;
        painter.circle_filled(
            at(*p),
            3.0 + 5.0 * share,
            accent.gamma_multiply(0.4 + 0.6 * share),
        );
    }
    if let Some(value) = value {
        let p: Pos2 = at(value);
        let cross = Stroke::new(2.0_f32, Color32::WHITE);
        painter.line_segment([p - vec2(5.0, 0.0), p + vec2(5.0, 0.0)], cross);
        painter.line_segment([p - vec2(0.0, 5.0), p + vec2(0.0, 5.0)], cross);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_directions_find_their_clips_by_name() {
        let clips: Vec<String> = [
            "Idle",
            "Walk Forward",
            "Strafe Left",
            "Strafe Right",
            "Walk Back",
            "Back Left",
            "Back Right",
            "Forward Left",
            "Forward Right",
        ]
        .iter()
        .map(|name| format!("assets/animations/hero/{name}.anim.json"))
        .collect();
        let mut doc = ControllerDoc::open(
            "assets/animations/hero/hero.statemachine.json".into(),
            StateMachineFile {
                states: vec![StateDef::new(
                    "Move",
                    Motion::Clip(String::new()),
                    [0.0, 0.0],
                )],
                ..StateMachineFile::default()
            },
            None,
        );
        doc.make_blend_space(0);
        assert_eq!(
            doc.file
                .parameters
                .iter()
                .map(|p| p.name())
                .collect::<Vec<_>>(),
            ["MoveX", "MoveZ"]
        );
        doc.add_eight_directions(0, &clips);
        let Motion::BlendSpace(space) = &doc.file.states[0].motion else {
            panic!("a blend space");
        };
        let at = |x: f64, y: f64| {
            let child = space
                .children
                .iter()
                .find(|c| c.position == [x, y])
                .unwrap();
            axcenity_anim::files::clip_name(&child.clip).to_owned()
        };
        assert_eq!(space.children.len(), 9);
        assert_eq!(at(0.0, 0.0), "Idle");
        assert_eq!(at(0.0, 1.0), "Walk Forward");
        assert_eq!(at(-1.0, 0.0), "Strafe Left");
        assert_eq!(at(1.0, -1.0), "Back Right");
        assert_eq!(at(-1.0, 1.0), "Forward Left");
        assert_eq!(at(0.0, -1.0), "Walk Back");
    }
}
