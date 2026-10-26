//! The UI Designer's edits to several elements at once: selecting them,
//! dragging the box around them, aligning, distributing and nudging them,
//! Paste style, and pasting copied elements. Duplicated and pasted
//! elements get fresh `Id`s and copies of their own rules. Each is one
//! undo step. These are methods of [`super::EditorApp`].

use std::collections::HashSet;

use axcenity_core::UDim2;

use super::*;
use crate::ui_designer::UiClipboard;
use crate::ui_designer::arrange::{OwnRules, element_id};

/// Every element `Id` used in `world`, in or out of the tree (an undone
/// insert can come back).
fn element_ids(world: &World) -> HashSet<String> {
    world
        .descendants(world.root())
        .into_iter()
        .filter_map(|id| element_id(world, id).map(str::to_owned))
        .collect()
}

/// `wanted` if no element has that `Id`, else a fresh one made from it.
fn unused_element_id(world: &World, wanted: &str) -> String {
    if element_ids(world).contains(wanted) {
        fresh_element_id(world, wanted)
    } else {
        wanted.to_owned()
    }
}

/// An `Id` no element has, for a copy of the element with `Id` `original`:
/// its letters with the lowest number after them that's free (`play2`,
/// `play3`, …).
fn fresh_element_id(world: &World, original: &str) -> String {
    let taken = element_ids(world);
    let base = original.trim_end_matches(|c: char| c.is_ascii_digit());
    let base = if base.is_empty() { "element" } else { base };
    (2..)
        .map(|n| format!("{base}{n}"))
        .find(|id| !taken.contains(id))
        .expect("there's always a free number")
}

/// One change per stylesheet: the text before the first and after the last.
fn merged(texts: Vec<history::TextChange>) -> Vec<history::TextChange> {
    let mut merged: Vec<history::TextChange> = Vec::new();
    for text in texts {
        match merged.iter_mut().find(|known| known.path == text.path) {
            Some(known) => known.after = text.after,
            None => merged.push(text),
        }
    }
    merged
}

/// "Play", or "3 elements".
fn named(world: &World, ids: &[InstanceId]) -> String {
    match ids {
        [only] => world
            .get(*only)
            .map_or_else(String::new, |instance| instance.name().to_owned()),
        _ => format!("{} elements", ids.len()),
    }
}

impl EditorApp {
    /// Selects `ids`, the last one the one Properties shows.
    pub(super) fn select_many(&mut self, ids: Vec<InstanceId>) {
        self.selected = ids.last().copied();
        self.selection = ids;
        if let Some(id) = self.selected {
            self.explorer.reveal(shown(&self.world, &self.play), id);
        }
    }

    /// The box around several elements dragged in the designer: every
    /// element's new `Position` and `Size`, and the CSS lengths rewritten,
    /// as one undo step for the whole drag.
    pub(super) fn apply_ui_drags(&mut self, resize: bool, edits: Vec<UiDragEdit>) {
        if self.playing() {
            return;
        }
        if edits.len() == 1 {
            return self.apply_ui_drag(edits.into_iter().next().expect("one"));
        }
        let ids: Vec<InstanceId> = edits
            .iter()
            .map(|edit| edit.id)
            .filter(|&id| self.world.contains(id))
            .collect();
        let before: Vec<_> = ids
            .iter()
            .map(|&id| history::snapshot(&self.world, id))
            .collect();
        let mut css = Vec::new();
        for edit in edits {
            for (property, value) in edit.values {
                if let Err(err) =
                    edits::set(&mut self.world, edit.id, Edit::Property(property, value))
                {
                    self.output.error(format!("Could not change that: {err:#}"));
                }
            }
            css.extend(edit.css);
        }
        let texts = self.drag_css_texts(css);
        let changed = ids
            .iter()
            .zip(before)
            .map(|(&id, before)| history::Changed {
                id,
                before,
                after: history::snapshot(&self.world, id),
            })
            .collect();
        let (key, verb) = if resize {
            (Field::Property("Size"), "Resize")
        } else {
            (Field::Property("Position"), "Move")
        };
        let label = format!("{verb} {}", named(&self.world, &ids));
        self.history
            .record_many_with_texts(&mut self.world, key, label, changed, texts);
    }

    /// Align, distribute or a nudge: each element's new `Position`, as one
    /// undo step named `label`.
    pub(super) fn arrange(&mut self, label: String, positions: Vec<(InstanceId, UDim2)>) {
        if positions.is_empty() {
            return;
        }
        self.history.seal();
        self.change_many(
            Field::Property("Position"),
            label,
            positions
                .into_iter()
                .map(|(id, position)| {
                    (id, vec![Edit::Property("Position", Value::UDim2(position))])
                })
                .collect(),
        );
        self.history.seal();
    }

    /// Paste style: each of `ids` gets `declarations` in its own `#Id`
    /// rule, changed in place where it has them, as one undo step. An
    /// element without an `Id` gets one from its name.
    pub(super) fn paste_style(
        &mut self,
        ids: Vec<InstanceId>,
        declarations: Vec<(String, String)>,
    ) {
        if self.playing() || self.project.is_none() {
            return;
        }
        let mut properties = Vec::new();
        let mut texts = Vec::new();
        for &id in &ids {
            let Some(instance) = self.world.get(id) else {
                continue;
            };
            let element_id = match element_id(&self.world, id) {
                Some(element_id) => element_id.to_owned(),
                None => {
                    let before = instance.property("Id").cloned();
                    let wanted = style_panel::id_from_name(instance.name());
                    let element_id = unused_element_id(&self.world, &wanted);
                    let after = Value::String(element_id.clone());
                    if let Err(err) = self.world.set_property(id, "Id", after.clone()) {
                        self.output.error(format!("Could not change that: {err}"));
                        continue;
                    }
                    properties.push(history::PropertyEdit {
                        id,
                        field: Field::Property("Id"),
                        before: before.unwrap_or_else(|| Value::String(String::new())),
                        after,
                    });
                    element_id
                }
            };
            let selector = style_panel::own_selector(&element_id);
            for (property, value) in &declarations {
                texts.extend(self.put_declaration(&selector, property, value));
            }
        }
        let label = format!("Paste style onto {}", named(&self.world, &ids));
        self.history
            .record_asset(&mut self.world, label, merged(texts), properties);
        self.history.seal();
    }

    /// Sets `property` to `value` in the rule written exactly as
    /// `selector`: in place when it has it, else added (the rule made if
    /// there isn't one). Returns the stylesheet's text before and after.
    fn put_declaration(
        &mut self,
        selector: &str,
        property: &str,
        value: &str,
    ) -> Option<history::TextChange> {
        let existing = self.project.as_ref().and_then(|project| {
            axcenity_ui::rule_declaration(&project.stylesheets, selector, property)
        });
        let Some(declaration) = existing else {
            return self.add_style_text(selector, property, value, false);
        };
        let source = self.stylesheet_text(&declaration.path)?;
        let edit = CssEdit {
            path: declaration.path.clone(),
            span: declaration.span,
            value: value.to_owned(),
        };
        let updated = apply_css_edits(&source, std::iter::once(&edit));
        self.set_stylesheet_text(&declaration.path, &updated);
        Some(history::TextChange {
            path: declaration.path,
            before: Some(source),
            after: Some(updated),
        })
    }

    /// Gives each copied element a fresh `Id` in place of `original`, with
    /// copies of `rules` (its original's own rules) for it. The copies are
    /// new, so the `Id`s go with them; returns the stylesheets' changes.
    fn give_fresh_ids(
        &mut self,
        copies: Vec<(InstanceId, String, OwnRules)>,
    ) -> Vec<history::TextChange> {
        let mut texts = Vec::new();
        for (id, original, rules) in copies {
            let fresh = fresh_element_id(&self.world, &original);
            if let Err(err) = self
                .world
                .set_property(id, "Id", Value::String(fresh.clone()))
            {
                self.output.error(format!("Could not change that: {err}"));
                continue;
            }
            for (suffix, declarations) in rules {
                let selector = format!("{}{suffix}", style_panel::own_selector(&fresh));
                for (property, value) in declarations {
                    texts.extend(self.put_declaration(&selector, &property, &value));
                }
            }
        }
        merged(texts)
    }

    /// Fresh `Id`s for every element with one in `roots` and inside them,
    /// copies of elements already in the project's stylesheets (Duplicate).
    pub(super) fn reid_duplicates(&mut self, roots: &[InstanceId]) -> Vec<history::TextChange> {
        let stylesheets = self
            .project
            .as_ref()
            .map_or(&[][..], |project| &project.stylesheets[..]);
        let copies = roots
            .iter()
            .flat_map(|&root| std::iter::once(root).chain(self.world.descendants(root)))
            .filter_map(|id| {
                let original = element_id(&self.world, id)?;
                let rules = axcenity_ui::own_rules(stylesheets, original);
                Some((id, original.to_owned(), rules))
            })
            .collect();
        self.give_fresh_ids(copies)
    }

    /// Ctrl+V in the designer: the copied elements go into `parent`, named
    /// apart from its children, with fresh `Id`s and copies of their own
    /// rules, as one undo step, and are selected.
    pub(super) fn paste_ui(&mut self, parent: InstanceId, clipboard: UiClipboard) {
        if self.playing() || self.project.is_none() {
            return;
        }
        let added = match axcenity_project::parse_prefab(&clipboard.prefab, &mut self.world, parent)
        {
            Ok(added) => added,
            Err(err) => {
                self.output.error(format!("Could not paste that: {err}"));
                return;
            }
        };
        let mut moves = Vec::new();
        for &id in &added {
            // Out of the tree for a moment, so its own name isn't "taken".
            let place = history::place_of(&self.world, id);
            let _ = history::put(&mut self.world, id, None);
            let wanted = self
                .world
                .get(id)
                .map(|instance| instance.name().to_owned())
                .unwrap_or_default();
            let name = tree::unique_name(&self.world, parent, &wanted);
            let _ = self.world.set_name(id, name);
            let _ = history::put(&mut self.world, id, place);
            moves.push(Move {
                id,
                before: None,
                after: place,
            });
        }
        let copies = added
            .iter()
            .flat_map(|&root| std::iter::once(root).chain(self.world.descendants(root)))
            .filter_map(|id| {
                let original = element_id(&self.world, id)?.to_owned();
                let rules = clipboard
                    .rules
                    .iter()
                    .find(|(copied, _)| *copied == original)
                    .map(|(_, rules)| rules.clone())
                    .unwrap_or_default();
                Some((id, original, rules))
            })
            .collect();
        let texts = self.give_fresh_ids(copies);
        let label = format!("Paste {}", named(&self.world, &added));
        self.history
            .record_files_and_tree(&mut self.world, label, texts, moves);
        self.history.seal();
        self.select_many(added);
    }
}
