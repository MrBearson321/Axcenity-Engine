use super::*;

impl PropertyDoc {
    /// Keys every value at the playhead, making the tracks it needs, and
    /// selects those keys.
    pub fn key_values(&mut self, values: &[EvaluatedProperty]) {
        self.key_values_from(values, |_| None);
    }

    /// [`Self::key_values`], where `rest` says what a property was before
    /// the clip moved it, so its first key after 0 also keys that at 0 and
    /// it moves.
    pub fn key_values_from(
        &mut self,
        values: &[EvaluatedProperty],
        rest: impl Fn(&EvaluatedProperty) -> Option<Vec<f32>>,
    ) {
        if self.read_only {
            return;
        }
        let time = self.snapped(self.playhead);
        let mut keyed = BTreeSet::new();
        for value in values {
            // A bone's first key after 0 starts from its rest pose.
            let start = if value.property == PropertyName::Bone {
                self.rig
                    .as_deref()
                    .and_then(|rig| rest_numbers(rig, &value.target))
            } else {
                rest(value)
            };
            let Some(key) = set_key(&mut self.file.tracks, value, time, self.new_keys, start)
            else {
                continue;
            };
            self.pending
                .retain(|p| !(p.target == value.target && p.property == value.property));
            self.selected_track = Some(key.0.clone());
            keyed.insert(key);
        }
        if !keyed.is_empty() {
            self.selected = keyed;
        }
    }

    /// Key selected (K): keys the selected object's tracks, its pending
    /// moves, and where it is (Position and Orientation, or a UI element's
    /// Position and Size). With a bone selected, it keys the bones the
    /// last move turned and hasn't keyed instead (every bone a joint moved
    /// with IK turned), if there are any.
    pub fn key_focus(&mut self, focus: &Focus) {
        if self.playing || self.playhead != self.pending_at {
            self.pending.clear();
        }
        let bone = focus
            .values
            .iter()
            .any(|v| v.property == PropertyName::Bone);
        let turned: Vec<EvaluatedProperty> = self
            .pending
            .iter()
            .filter(|p| p.property == PropertyName::Bone)
            .cloned()
            .collect();
        if bone && !turned.is_empty() {
            self.key_values(&turned);
            return;
        }
        let mut properties: Vec<PropertyName> = focus.transform.clone();
        let more = self
            .file
            .tracks
            .iter()
            .filter(|t| t.target == focus.target)
            .map(|t| t.property)
            .chain(
                self.pending
                    .iter()
                    .filter(|p| p.target == focus.target)
                    .map(|p| p.property),
            );
        for property in more {
            if !properties.contains(&property) {
                properties.push(property);
            }
        }
        let values: Vec<EvaluatedProperty> = properties
            .into_iter()
            .filter_map(|property| {
                self.pending
                    .iter()
                    .chain(&focus.values)
                    .find(|v| v.target == focus.target && v.property == property)
                    .cloned()
            })
            .collect();
        self.key_values(&values);
    }

    /// Gives every selected key `interp` (and `easing`, for an eased one).
    /// A Bezier key keeps its handles.
    pub fn ease_selected(&mut self, interp: Interp, easing: Option<Easing>) {
        if self.read_only {
            return;
        }
        for track in &mut self.file.tracks {
            for key in &mut track.keys {
                if !self.selected.contains(&(track.id.clone(), key.id.clone())) {
                    continue;
                }
                key.interp = interp;
                key.easing = easing.filter(|_| interp == Interp::Ease);
                if interp != Interp::Bezier {
                    key.bezier = None;
                }
            }
        }
    }

    /// Easing ▸, for the selected keys' right-click menu: Smooth, Linear
    /// and Constant, then each eased style and Bezier.
    pub(super) fn easing_menu(&mut self, ui: &mut egui::Ui) {
        let count = self.selected.len();
        crate::widgets::menu_button(ui, Some(icons::WAVE_SINE), "Easing", |ui| {
            ui.weak(if count == 1 {
                "For the selected key".to_owned()
            } else {
                format!("For the {count} selected keys")
            });
            for choice in NewKeys::ALL {
                if ui.button(choice.label()).clicked() {
                    let (interp, easing) = choice.easing();
                    self.ease_selected(interp, easing);
                    ui.close();
                }
            }
            ui.separator();
            for direction in EasingDirection::ALL {
                crate::widgets::menu_button(
                    ui,
                    None,
                    &format!("Ease {}", direction.as_str()),
                    |ui| {
                        for style in EasingStyle::ALL {
                            if ui.button(style.as_str()).clicked() {
                                self.ease_selected(
                                    Interp::Ease,
                                    Some(Easing::new(style, direction)),
                                );
                                ui.close();
                            }
                        }
                    },
                );
            }
            if ui.button("Bezier").clicked() {
                self.ease_selected(Interp::Bezier, None);
                ui.close();
            }
        });
    }

    pub fn delete_selected(&mut self) {
        if self.read_only {
            return;
        }
        for (track_id, key_id) in std::mem::take(&mut self.selected) {
            if let Some(track) = self.file.tracks.iter_mut().find(|t| t.id == track_id) {
                track.keys.retain(|k| k.id != key_id);
            }
        }
    }

    pub fn delete_track(&mut self, id: &str) {
        if self.read_only {
            return;
        }
        self.file.tracks.retain(|t| t.id != id);
        self.selected.retain(|(track, _)| track != id);
        if self.selected_track.as_deref() == Some(id) {
            self.selected_track = None;
        }
    }

    /// The selected keys, and their times.
    pub(super) fn selected_keys(&self) -> Vec<(KeyRef, f64)> {
        self.file
            .tracks
            .iter()
            .flat_map(|t| t.keys.iter().map(move |k| (t, k)))
            .filter(|(t, k)| self.selected.contains(&(t.id.clone(), k.id.clone())))
            .map(|(t, k)| ((t.id.clone(), k.id.clone()), k.time))
            .collect()
    }

    pub fn copy(&mut self) {
        let keys = self.selected_keys();
        let Some(first) = keys.iter().map(|(_, time)| *time).min_by(f64::total_cmp) else {
            return;
        };
        self.clipboard = keys
            .into_iter()
            .filter_map(|((track_id, key_id), time)| {
                let track = self.file.tracks.iter().find(|t| t.id == track_id)?;
                let key = track.keys.iter().find(|k| k.id == key_id)?;
                Some((track_id, time - first, key.clone()))
            })
            .collect();
    }

    pub fn can_paste(&self) -> bool {
        !self.clipboard.is_empty()
    }

    /// Pastes the copied keys at the playhead, on the tracks they came
    /// from, replacing keys at the same times; then selects them.
    pub fn paste(&mut self) {
        if self.read_only {
            return;
        }
        let mut pasted = BTreeSet::new();
        for (track_id, offset, key) in self.clipboard.clone() {
            let time = self.snapped(self.playhead + offset);
            let Some(track) = self.file.tracks.iter_mut().find(|t| t.id == track_id) else {
                continue;
            };
            track.keys.retain(|k| (k.time - time).abs() >= 1e-6);
            let id = free_id("k", track.keys.iter().map(|k| k.id.as_str()));
            track.keys.push(PropertyKeyframe {
                id: id.clone(),
                time,
                ..key
            });
            track.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
            pasted.insert((track_id, id));
        }
        if !pasted.is_empty() {
            self.selected = pasted;
        }
    }

    /// After keys moved: sorted again, and where a moved key landed on
    /// another, the moved one stays.
    pub(super) fn tidy(&mut self) {
        for track in &mut self.file.tracks {
            let moved = |key: &PropertyKeyframe| {
                self.selected.contains(&(track.id.clone(), key.id.clone()))
            };
            let mut keys = std::mem::take(&mut track.keys);
            keys.sort_by(|a, b| a.time.total_cmp(&b.time).then(moved(a).cmp(&moved(b))));
            let mut kept: Vec<PropertyKeyframe> = Vec::with_capacity(keys.len());
            for key in keys {
                if kept
                    .last()
                    .is_some_and(|last| (last.time - key.time).abs() < 1e-6)
                {
                    *kept.last_mut().expect("just checked") = key;
                } else {
                    kept.push(key);
                }
            }
            track.keys = kept;
        }
        let tracks = &self.file.tracks;
        self.selected.retain(|(track_id, key_id)| {
            tracks
                .iter()
                .any(|t| t.id == *track_id && t.keys.iter().any(|k| k.id == *key_id))
        });
    }
}

/// Keys `value` at `time` in `tracks`, making its track if there's none
/// (`bone1`, `position1`…), or replacing the value of a key already at that
/// time (which keeps its easing). A new key eases as `new_keys` says.
/// Returns the track's and key's ids, or `None` if the value doesn't fit
/// the track (a UDim2 key can't go on a Vector3 track, or the reverse).
/// Where the bone `target` names rests in `rig`, as a `Bone` key's numbers.
pub(in crate::animation) fn rest_numbers(rig: &Rig, target: &str) -> Option<Vec<f32>> {
    let bone = find_bone(rig, &bone_paths(rig), target)?;
    Some(bone_numbers(&rig.bones.get(bone)?.rest))
}

/// Keys `value` at `time`, making its track if needed. With `start`, a
/// track's first key after 0 also gets a key at 0 with `start`'s numbers
/// (where it was before), so the move plays from there.
pub(in crate::animation) fn set_key(
    tracks: &mut Vec<PropertyTrack>,
    value: &EvaluatedProperty,
    time: f64,
    new_keys: NewKeys,
    start: Option<Vec<f32>>,
) -> Option<KeyRef> {
    let index = match tracks
        .iter()
        .position(|t| t.target == value.target && t.property == value.property)
    {
        Some(index) => index,
        None => {
            let id = free_id(
                &value.property.as_str().to_lowercase(),
                tracks.iter().map(|t| t.id.as_str()),
            );
            tracks.push(PropertyTrack {
                id,
                target: value.target.clone(),
                property: value.property,
                keys: Vec::new(),
            });
            tracks.len() - 1
        }
    };
    let track = &mut tracks[index];
    if track
        .keys
        .first()
        .is_some_and(|key| key.value.len() != value.value.len())
    {
        return None;
    }
    if track.keys.is_empty()
        && time > 1e-6
        && let Some(start) = start.filter(|start| start.len() == value.value.len())
    {
        let (interp, easing) = new_keys.easing();
        track.keys.push(PropertyKeyframe {
            id: free_id("k", std::iter::empty()),
            time: 0.0,
            value: start,
            interp,
            easing,
            bezier: None,
        });
    }
    let id = match track.keys.iter_mut().find(|k| (k.time - time).abs() < 1e-6) {
        Some(key) => {
            key.value = value.value.clone();
            key.id.clone()
        }
        None => {
            let id = free_id("k", track.keys.iter().map(|k| k.id.as_str()));
            let (interp, easing) = new_keys.easing();
            track.keys.push(PropertyKeyframe {
                id: id.clone(),
                time,
                value: value.value.clone(),
                interp,
                easing,
                bezier: None,
            });
            track.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
            id
        }
    };
    Some((track.id.clone(), id))
}
