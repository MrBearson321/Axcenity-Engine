//! The Curves view: the shown tracks' numbers as curves over time, drawn
//! by sampling them as the game plays them (`anim::sample_track`).
//!
//! Drag a key to move it in time and value, and a Bezier key's handles to
//! shape its motion to the next key; they show while either key of that
//! gap is selected. Drag empty space (or with the middle button anywhere)
//! to pan. The wheel zooms around the mouse: Shift zooms time only, Ctrl
//! values only. Frame all (F) fits the clip and its curves.
//!
//! A bone's track shows its turn as X, Y and Z angles in degrees (and
//! where it is, for a bone whose keys move it), never its raw quaternion
//! or scale, so dragging a curve only turns the bone.

use axcenity_anim::{DEFAULT_BEZIER, sample_track};

use super::*;

/// Curve colours, one per number of a key: X, Y and Z (as the gizmo
/// draws its axes), then the rest of a UDim2's or a bone's numbers.
const COLORS: [egui::Color32; 10] = [
    egui::Color32::from_rgb(235, 65, 65),
    egui::Color32::from_rgb(70, 205, 70),
    egui::Color32::from_rgb(55, 125, 245),
    egui::Color32::from_rgb(230, 180, 40),
    egui::Color32::from_rgb(245, 120, 120),
    egui::Color32::from_rgb(130, 225, 130),
    egui::Color32::from_rgb(120, 170, 250),
    egui::Color32::from_rgb(200, 120, 230),
    egui::Color32::from_rgb(60, 200, 200),
    egui::Color32::from_rgb(240, 150, 60),
];

/// The left column of track names, in points.
const LABELS: f32 = 120.0;

/// What part of the visible range the Curves view shows, and where it
/// last drew.
#[derive(Clone, Debug)]
pub struct CurveView {
    /// Seconds at the left and right edges.
    pub time: (f64, f64),
    /// Values at the bottom and top edges.
    pub value: (f32, f32),
    /// The tracks it was last framed for; showing others frames again.
    framed_for: Option<Vec<String>>,
    /// Where the curves were last drawn.
    pub rect: egui::Rect,
}

impl Default for CurveView {
    fn default() -> Self {
        Self {
            time: (0.0, 1.0),
            value: (-1.0, 1.0),
            framed_for: None,
            rect: egui::Rect::NOTHING,
        }
    }
}

impl CurveView {
    /// Where `(time, value)` is on screen.
    pub fn screen(&self, time: f64, value: f32) -> egui::Pos2 {
        let (t0, t1) = self.time;
        let (v0, v1) = self.value;
        let x = self.rect.left() + ((time - t0) / (t1 - t0).max(1e-9)) as f32 * self.rect.width();
        let y = self.rect.bottom() - (value - v0) / (v1 - v0).max(1e-9) * self.rect.height();
        egui::pos2(x, y)
    }

    /// The `(time, value)` under `pos`.
    pub fn at(&self, pos: egui::Pos2) -> (f64, f32) {
        let (t0, t1) = self.time;
        let (v0, v1) = self.value;
        let time =
            t0 + ((pos.x - self.rect.left()) / self.rect.width().max(1.0)) as f64 * (t1 - t0);
        let value = v0 + (self.rect.bottom() - pos.y) / self.rect.height().max(1.0) * (v1 - v0);
        (time, value)
    }

    /// Zooms by `factor` (below 1 zooms in) around `pos`, in time, values
    /// or both.
    fn zoom(&mut self, pos: egui::Pos2, factor: f32, time: bool, value: bool) {
        let (t, v) = self.at(pos);
        if time {
            let f = f64::from(factor);
            self.time = (t + (self.time.0 - t) * f, t + (self.time.1 - t) * f);
        }
        if value {
            self.value = (
                v + (self.value.0 - v) * factor,
                v + (self.value.1 - v) * factor,
            );
        }
    }
}

/// A key or handle in the Curves view: its track, its key, and which of
/// the key's numbers it's drawn for.
#[derive(Clone, Debug, PartialEq)]
enum Grab {
    Key {
        key: KeyRef,
        number: usize,
    },
    /// One of the handles of the Bezier gap that starts at `key`: `0` by
    /// it, `1` by the next key.
    Handle {
        key: KeyRef,
        handle: usize,
        number: usize,
    },
}

/// What a drag in the Curves view is doing.
#[derive(Clone, Debug)]
pub(super) enum CurveGesture {
    Pan {
        time: (f64, f64),
        value: (f32, f32),
    },
    /// Moving a key: the time and value under the mouse when it went down,
    /// and the key's then.
    Key {
        key: KeyRef,
        number: usize,
        from: (f64, f32),
        start: (f64, f32),
    },
    /// Moving a handle of the gap from key `a` to the next: the gap's
    /// times and this number's values at its ends.
    Handle {
        key: KeyRef,
        handle: usize,
        times: (f64, f64),
        values: (f32, f32),
    },
}

impl PropertyDoc {
    /// The tracks the Curves view shows: those picked in its list, or the
    /// selected track, or the first.
    pub fn curve_tracks(&self) -> Vec<String> {
        let picked: Vec<String> = self
            .file
            .tracks
            .iter()
            .filter(|t| self.curve_picked.contains(&t.id))
            .map(|t| t.id.clone())
            .collect();
        if !picked.is_empty() {
            return picked;
        }
        self.selected_track
            .iter()
            .filter(|id| self.file.tracks.iter().any(|t| &t.id == *id))
            .cloned()
            .chain(self.file.tracks.first().map(|t| t.id.clone()))
            .take(1)
            .collect()
    }

    /// Fits the clip's length and the shown curves into the view.
    pub fn frame_all(&mut self) {
        let shown = self.curve_tracks();
        let duration = self.file.duration.max(0.001);
        let (mut low, mut high) = (f32::INFINITY, f32::NEG_INFINITY);
        if let Ok(clip) = self.clip() {
            for track in clip.file.tracks.iter().filter(|t| shown.contains(&t.id)) {
                for step in 0..=200 {
                    let time = duration * f64::from(step) / 200.0;
                    let sampled = sample_track(track, time).unwrap_or_default();
                    for n in curve_numbers(track, &sampled) {
                        low = low.min(n);
                        high = high.max(n);
                    }
                }
                for (index, key) in track.keys.iter().enumerate() {
                    let shown = curve_numbers(track, &key.value);
                    for n in &shown {
                        low = low.min(*n);
                        high = high.max(*n);
                    }
                    // Bezier handles, so they can be grabbed.
                    if key.interp == Interp::Bezier
                        && let Some(next) = track.keys.get(index + 1)
                    {
                        let [_, y1, _, y2] = key.bezier.unwrap_or(DEFAULT_BEZIER);
                        for (a, b) in shown.iter().zip(&curve_numbers(track, &next.value)) {
                            for y in [y1, y2] {
                                let n = a + y * (b - a);
                                low = low.min(n);
                                high = high.max(n);
                            }
                        }
                    }
                }
            }
        }
        if !low.is_finite() {
            (low, high) = (-1.0, 1.0);
        }
        if high - low < 1e-3 {
            (low, high) = (low - 1.0, high + 1.0);
        }
        let pad = (high - low) * 0.1;
        self.curve_view.value = (low - pad, high + pad);
        let pad = duration * 0.04;
        self.curve_view.time = (-pad, duration + pad);
        self.curve_view.framed_for = Some(shown);
    }

    /// The Curves view, in place of the dope sheet.
    pub(super) fn curves(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(ui.ctx());
        let height = ui.available_height().clamp(160.0, 420.0);
        let (outer, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            egui::Sense::click_and_drag(),
        );
        let painter = ui.painter_at(outer);
        painter.rect_filled(outer, 0.0, palette.inset);
        let label_w = LABELS.min(outer.width() * 0.35);
        self.curve_view.rect = egui::Rect::from_min_max(
            egui::pos2(outer.left() + label_w + 6.0, outer.top() + RULER),
            egui::pos2(outer.right() - 8.0, outer.bottom() - 6.0),
        );
        let shown = self.curve_tracks();
        if self.curve_view.framed_for.as_ref() != Some(&shown) {
            self.frame_all();
        }
        let view = self.curve_view.clone();
        let area = view.rect;

        // The track list: click to show one, Shift or Ctrl to add it.
        let (modifiers, origin, pointer) =
            ui.input(|i| (i.modifiers, i.pointer.press_origin(), i.pointer.hover_pos()));
        let add = modifiers.shift || modifiers.command;
        let label_at = |pos: egui::Pos2| -> Option<usize> {
            if pos.x >= area.left() - 6.0 {
                return None;
            }
            let row = ((pos.y - outer.top() - RULER) / ROW).floor();
            (row >= 0.0).then_some(row as usize)
        };
        for (row, track) in self.file.tracks.iter().enumerate() {
            let y = outer.top() + RULER + row as f32 * ROW;
            let row_rect =
                egui::Rect::from_min_size(egui::pos2(outer.left(), y), egui::vec2(label_w, ROW));
            let on = shown.contains(&track.id);
            if on {
                painter.rect_filled(row_rect, 0.0, palette.accent_soft);
            }
            let name = match track.target.as_str() {
                "." | "" => "(root)",
                path => path.rsplit('/').next().unwrap_or(path),
            };
            painter.with_clip_rect(row_rect).text(
                egui::pos2(row_rect.left() + 4.0, y + ROW / 2.0),
                egui::Align2::LEFT_CENTER,
                format!("{name} · {}", track.property.as_str()),
                egui::FontId::proportional(12.0),
                if on { palette.text } else { palette.text_weak },
            );
        }

        // Grid: time along the top, values up the side.
        let painter_area = painter.with_clip_rect(area.expand2(egui::vec2(0.0, RULER)));
        let step = nice_step(view.time.1 - view.time.0, area.width());
        let mut t = (view.time.0 / step).ceil() * step;
        while t <= view.time.1 {
            let x = view.screen(t, 0.0).x;
            painter_area.line_segment(
                [egui::pos2(x, area.top()), egui::pos2(x, area.bottom())],
                egui::Stroke::new(1.0_f32, palette.border),
            );
            painter_area.text(
                egui::pos2(x + 2.0, outer.top() + 2.0),
                egui::Align2::LEFT_TOP,
                trim(t),
                egui::FontId::proportional(10.0),
                palette.text_weak,
            );
            t += step;
        }
        let span = f64::from(view.value.1 - view.value.0);
        let rows = f64::from((area.height() / 28.0).max(1.0));
        let value_step = nice_number(span / rows);
        let mut v = (f64::from(view.value.0) / value_step).ceil() * value_step;
        while v <= f64::from(view.value.1) {
            let y = view.screen(0.0, v as f32).y;
            painter_area.line_segment(
                [egui::pos2(area.left(), y), egui::pos2(area.right(), y)],
                egui::Stroke::new(1.0_f32, palette.border),
            );
            painter_area.text(
                egui::pos2(area.left() + 2.0, y - 1.0),
                egui::Align2::LEFT_BOTTOM,
                trim(v),
                egui::FontId::proportional(10.0),
                palette.text_weak,
            );
            v += value_step;
        }
        // The clip's ends.
        for end in [0.0, self.file.duration] {
            let x = view.screen(end, 0.0).x;
            painter_area.line_segment(
                [egui::pos2(x, area.top()), egui::pos2(x, area.bottom())],
                egui::Stroke::new(1.0_f32, palette.border_strong),
            );
        }

        // Curves, keys and handles, as the clip plays them.
        let clip = self.clip();
        let mut grabs: Vec<(Grab, egui::Pos2)> = Vec::new();
        let curves = painter.with_clip_rect(area);
        match &clip {
            Ok(clip) => {
                for track in clip.file.tracks.iter().filter(|t| shown.contains(&t.id)) {
                    self.draw_track(&curves, &view, track, &mut grabs, &palette);
                }
            }
            Err(problem) => {
                painter.text(
                    area.left_top() + egui::vec2(4.0, 4.0),
                    egui::Align2::LEFT_TOP,
                    problem,
                    egui::FontId::proportional(12.0),
                    palette.error,
                );
            }
        }
        if self.file.tracks.is_empty() {
            painter.text(
                area.left_center(),
                egui::Align2::LEFT_CENTER,
                "No tracks yet: select something in the preview, move it, and press K.",
                egui::FontId::proportional(12.0),
                palette.text_weak,
            );
        }
        let grab_at = |pos: egui::Pos2| {
            grabs
                .iter()
                .map(|(grab, at)| (grab, at.distance(pos)))
                .filter(|(_, d)| *d <= GRAB)
                // Handles sit over their keys, so they win a tie.
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(grab, _)| grab.clone())
        };

        // Zoom.
        if let Some(pos) = pointer.filter(|p| area.contains(*p))
            && response.hovered()
        {
            let (scroll, zoom, frame) = ui.input(|i| {
                (
                    i.smooth_scroll_delta.x + i.smooth_scroll_delta.y,
                    i.zoom_delta(),
                    i.key_pressed(egui::Key::F) && i.modifiers.is_none(),
                )
            });
            if scroll != 0.0 {
                let factor = (-scroll * 0.003).exp();
                self.curve_view
                    .zoom(pos, factor, !modifiers.command, !modifiers.shift);
            }
            if zoom != 1.0 {
                self.curve_view.zoom(pos, 1.0 / zoom, false, true);
            }
            if frame {
                self.frame_all();
            }
        }

        // Drags.
        if response.drag_started()
            && let Some(origin) = origin
        {
            let middle = ui.input(|i| i.pointer.middle_down());
            let pan = CurveGesture::Pan {
                time: view.time,
                value: view.value,
            };
            self.curve_gesture = if middle || !area.contains(origin) {
                area.contains(origin).then_some(pan)
            } else {
                match grab_at(origin) {
                    Some(Grab::Key { key, number }) => {
                        if !self.selected.contains(&key) {
                            if !add {
                                self.selected.clear();
                            }
                            self.selected.insert(key.clone());
                        }
                        self.find_key(&key)
                            // Read-only keys are selected, not moved.
                            .filter(|_| !self.read_only)
                            .map(|k| CurveGesture::Key {
                                number,
                                from: view.at(origin),
                                start: (k.time, self.shown_number(&key, number)),
                                key,
                            })
                    }
                    Some(Grab::Handle { .. }) if self.read_only => None,
                    Some(Grab::Handle {
                        key,
                        handle,
                        number,
                    }) => self
                        .gap(&key, number)
                        .map(|(times, values)| CurveGesture::Handle {
                            key,
                            handle,
                            times,
                            values,
                        }),
                    None => Some(pan),
                }
            };
        }
        if let (Some(pos), true) = (response.interact_pointer_pos(), response.dragged()) {
            match self.curve_gesture.clone() {
                Some(CurveGesture::Pan { time, value }) => {
                    let origin = origin.unwrap_or(pos);
                    let dt =
                        f64::from((pos.x - origin.x) / area.width().max(1.0)) * (time.1 - time.0);
                    let dv = (pos.y - origin.y) / area.height().max(1.0) * (value.1 - value.0);
                    self.curve_view.time = (time.0 - dt, time.1 - dt);
                    self.curve_view.value = (value.0 + dv, value.1 + dv);
                }
                Some(CurveGesture::Key {
                    key,
                    number,
                    from,
                    start,
                }) => {
                    let (t, v) = view.at(pos);
                    let time = self.snapped(start.0 + t - from.0);
                    let property = self.track_of(&key).map(|t| t.property);
                    if let (Some(k), Some(property)) = (self.find_key_mut(&key), property) {
                        k.time = time;
                        set_shown_number(property, &mut k.value, number, start.1 + v - from.1);
                    }
                }
                Some(CurveGesture::Handle {
                    key,
                    handle,
                    times,
                    values,
                }) => {
                    let (t, v) = view.at(pos);
                    let x = ((t - times.0) / (times.1 - times.0).max(1e-9)).clamp(0.0, 1.0);
                    let y = (v - values.0) / (values.1 - values.0);
                    if let Some(k) = self.find_key_mut(&key) {
                        let handles = k.bezier.get_or_insert(DEFAULT_BEZIER);
                        let at = handle * 2;
                        handles[at] = (x as f32 * 1000.0).round() / 1000.0;
                        handles[at + 1] = (y * 1000.0).round() / 1000.0;
                    }
                }
                None => {}
            }
        }
        if response.drag_stopped()
            && let Some(CurveGesture::Key { .. }) = self.curve_gesture.take()
        {
            self.tidy();
        }

        if response.secondary_clicked()
            && let Some(pos) = response.interact_pointer_pos()
            && let Some(Grab::Key { key, .. }) = grab_at(pos)
            && !self.selected.contains(&key)
        {
            self.selected = BTreeSet::from([key]);
        }
        if !self.read_only && !self.selected.is_empty() {
            response.context_menu(|ui| self.easing_menu(ui));
        }
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            if let Some(row) = label_at(pos)
                && let Some(track) = self.file.tracks.get(row)
            {
                let id = track.id.clone();
                if add {
                    if !self.curve_picked.remove(&id) {
                        self.curve_picked.insert(id.clone());
                    }
                } else {
                    self.curve_picked = BTreeSet::from([id.clone()]);
                }
                self.selected_track = Some(id);
            } else if area.contains(pos) {
                match grab_at(pos) {
                    Some(Grab::Key { key, .. }) if add => {
                        if !self.selected.remove(&key) {
                            self.selected.insert(key);
                        }
                    }
                    Some(Grab::Key { key, .. }) => {
                        self.selected_track = Some(key.0.clone());
                        self.selected = BTreeSet::from([key]);
                    }
                    Some(Grab::Handle { .. }) => {}
                    None => {
                        if !add {
                            self.selected.clear();
                        }
                        self.playhead = self.snapped(view.at(pos).0);
                        self.playing = false;
                    }
                }
            }
        }

        // Playhead.
        let px = view.screen(self.playhead, 0.0).x;
        painter_area.line_segment(
            [egui::pos2(px, outer.top()), egui::pos2(px, area.bottom())],
            egui::Stroke::new(2.0_f32, palette.error),
        );
    }

    /// Draws `track`'s curves, keys and the handles of its selected Bezier
    /// gaps, adding what can be grabbed to `grabs`.
    fn draw_track(
        &self,
        painter: &egui::Painter,
        view: &CurveView,
        track: &PropertyTrack,
        grabs: &mut Vec<(Grab, egui::Pos2)>,
        palette: &theme::Palette,
    ) {
        let width = track
            .keys
            .first()
            .map_or(0, |k| curve_numbers(track, &k.value).len());
        // Sample every two points, and on each key so corners are sharp.
        let samples = (view.rect.width() / 2.0).max(2.0) as usize;
        let mut times: Vec<f64> = (0..=samples)
            .map(|i| view.time.0 + (view.time.1 - view.time.0) * i as f64 / samples as f64)
            .chain(track.keys.iter().flat_map(|k| [k.time - 1e-6, k.time]))
            .collect();
        times.sort_by(f64::total_cmp);
        let values: Vec<Vec<f32>> = times
            .iter()
            .map(|t| curve_numbers(track, &sample_track(track, *t).unwrap_or_default()))
            .collect();
        let shown: Vec<Vec<f32>> = track
            .keys
            .iter()
            .map(|k| curve_numbers(track, &k.value))
            .collect();
        for number in 0..width {
            let color = COLORS[number % COLORS.len()];
            // An angle wrapping round from 180 to -180 degrees starts a new
            // line.
            let wraps = track.property == PropertyName::Bone && number < 3;
            let mut line: Vec<egui::Pos2> = Vec::new();
            let mut last: Option<f32> = None;
            for (t, v) in times.iter().zip(&values) {
                let Some(&v) = v.get(number) else { continue };
                if wraps && last.is_some_and(|last| (v - last).abs() > 180.0) {
                    let done = std::mem::take(&mut line);
                    painter.add(egui::Shape::line(done, egui::Stroke::new(1.5_f32, color)));
                }
                line.push(view.screen(*t, v));
                last = Some(v);
            }
            painter.add(egui::Shape::line(line, egui::Stroke::new(1.5_f32, color)));
        }
        for (index, key) in track.keys.iter().enumerate() {
            let key_ref = (track.id.clone(), key.id.clone());
            let selected = self.selected.contains(&key_ref);
            // A Bezier gap's handles show while either of its keys is.
            if key.interp == Interp::Bezier
                && let Some(next) = track.keys.get(index + 1)
                && (selected || self.selected.contains(&(track.id.clone(), next.id.clone())))
            {
                let [x1, y1, x2, y2] = key.bezier.unwrap_or(DEFAULT_BEZIER);
                let span = next.time - key.time;
                for number in 0..width {
                    let (a, b) = (shown[index][number], shown[index + 1][number]);
                    if (b - a).abs() < 1e-6 {
                        continue;
                    }
                    let color = COLORS[number % COLORS.len()];
                    for (handle, (hx, hy), from) in [
                        (0, (x1, y1), view.screen(key.time, a)),
                        (1, (x2, y2), view.screen(next.time, b)),
                    ] {
                        let at = view.screen(key.time + f64::from(hx) * span, a + hy * (b - a));
                        painter.line_segment([from, at], egui::Stroke::new(1.0_f32, color));
                        painter.circle(at, 4.0, palette.inset, egui::Stroke::new(1.5_f32, color));
                        grabs.push((
                            Grab::Handle {
                                key: key_ref.clone(),
                                handle,
                                number,
                            },
                            at,
                        ));
                    }
                }
            }
            for number in 0..width {
                let Some(&value) = shown[index].get(number) else {
                    continue;
                };
                let center = view.screen(key.time, value);
                let (fill, r) = if selected {
                    (palette.accent, 5.5)
                } else {
                    (COLORS[number % COLORS.len()], 4.5)
                };
                let diamond = vec![
                    center + egui::vec2(0.0, -r),
                    center + egui::vec2(r, 0.0),
                    center + egui::vec2(0.0, r),
                    center + egui::vec2(-r, 0.0),
                ];
                painter.add(egui::Shape::convex_polygon(
                    diamond,
                    fill,
                    egui::Stroke::new(1.0_f32, palette.inset),
                ));
                grabs.push((
                    Grab::Key {
                        key: key_ref.clone(),
                        number,
                    },
                    center,
                ));
            }
        }
    }

    fn track_of(&self, (track, _): &KeyRef) -> Option<&PropertyTrack> {
        self.file.tracks.iter().find(|t| &t.id == track)
    }

    /// The `number`th of the numbers the view shows for `key`.
    fn shown_number(&self, key: &KeyRef, number: usize) -> f32 {
        match (self.track_of(key), self.find_key(key)) {
            (Some(track), Some(k)) => curve_numbers(track, &k.value)
                .get(number)
                .copied()
                .unwrap_or(0.0),
            _ => 0.0,
        }
    }

    fn find_key(&self, (track, key): &KeyRef) -> Option<&PropertyKeyframe> {
        self.file
            .tracks
            .iter()
            .find(|t| &t.id == track)
            .and_then(|t| t.keys.iter().find(|k| &k.id == key))
    }

    fn find_key_mut(&mut self, (track, key): &KeyRef) -> Option<&mut PropertyKeyframe> {
        self.file
            .tracks
            .iter_mut()
            .find(|t| &t.id == track)
            .and_then(|t| t.keys.iter_mut().find(|k| &k.id == key))
    }

    /// The gap from `key` to the next key on its track: their times, and
    /// their values of `number`.
    fn gap(&self, (track, key): &KeyRef, number: usize) -> Option<((f64, f64), (f32, f32))> {
        let track = self.file.tracks.iter().find(|t| &t.id == track)?;
        let mut keys: Vec<&PropertyKeyframe> = track.keys.iter().collect();
        keys.sort_by(|a, b| a.time.total_cmp(&b.time));
        let index = keys.iter().position(|k| &k.id == key)?;
        let (a, b) = (keys[index], keys.get(index + 1)?);
        Some((
            (a.time, b.time),
            (
                *curve_numbers(track, &a.value).get(number)?,
                *curve_numbers(track, &b.value).get(number)?,
            ),
        ))
    }
}

/// A key's numbers as the view draws them: its [`shown_numbers`], leaving
/// out where a bone is unless its keys move it.
fn curve_numbers(track: &PropertyTrack, value: &[f32]) -> Vec<f32> {
    let mut shown = shown_numbers(track.property, value);
    if track.property == PropertyName::Bone && !bone_moves(track) {
        shown.truncate(3);
    }
    shown
}

/// Whether a bone track's keys put the bone in more than one place.
fn bone_moves(track: &PropertyTrack) -> bool {
    let Some(first) = track.keys.first() else {
        return false;
    };
    track.keys.iter().any(|key| {
        key.value
            .iter()
            .zip(&first.value)
            .take(3)
            .any(|(a, b)| (a - b).abs() > 1e-4)
    })
}

/// A step of 1, 2 or 5 times a power of ten near `rough`.
fn nice_number(rough: f64) -> f64 {
    let rough = rough.max(1e-6);
    let power = 10f64.powf(rough.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|m| m * power)
        .find(|step| *step >= rough)
        .unwrap_or(10.0 * power)
}

/// `n` without trailing zeros.
fn trim(n: f64) -> String {
    let text = format!("{:.3}", n + 0.0);
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}
