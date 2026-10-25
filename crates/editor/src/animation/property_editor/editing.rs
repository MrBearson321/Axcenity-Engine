use super::*;

impl PropertyDoc {
    /// Keys every value at the playhead, making the tracks it needs, and
    /// selects those keys.
    pub fn key_values(&mut self, values: &[EvaluatedProperty]) {
        let time = self.snapped(self.playhead);
        let mut keyed = BTreeSet::new();
        for value in values {
            let Some(key) = set_key(&mut self.file.tracks, value, time) else {
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
    /// Position and Size).
    pub fn key_focus(&mut self, focus: &Focus) {
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

    pub fn delete_selected(&mut self) {
        for (track_id, key_id) in std::mem::take(&mut self.selected) {
            if let Some(track) = self.file.tracks.iter_mut().find(|t| t.id == track_id) {
                track.keys.retain(|k| k.id != key_id);
            }
        }
    }

    pub fn delete_track(&mut self, id: &str) {
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
/// (`bone1`, `position1`…), or replacing a key already at that time.
/// Returns the track's and key's ids, or `None` if the value doesn't fit
/// the track (a UDim2 key can't go on a Vector3 track, or the reverse).
pub(in crate::animation) fn set_key(
    tracks: &mut Vec<PropertyTrack>,
    value: &EvaluatedProperty,
    time: f64,
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
    let id = match track.keys.iter_mut().find(|k| (k.time - time).abs() < 1e-6) {
        Some(key) => {
            key.value = value.value.clone();
            key.id.clone()
        }
        None => {
            let id = free_id("k", track.keys.iter().map(|k| k.id.as_str()));
            track.keys.push(PropertyKeyframe {
                id: id.clone(),
                time,
                value: value.value.clone(),
                interp: Interp::Linear,
            });
            track.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
            id
        }
    };
    Some((track.id.clone(), id))
}
