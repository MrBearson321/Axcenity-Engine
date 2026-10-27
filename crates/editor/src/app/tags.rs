//! The Tags panel's actions: selecting everything with a tag, and renaming
//! a tag across the world as one undo step.

use super::*;
use crate::tags::TagAction;

impl EditorApp {
    pub(in crate::app) fn tags_pane(&mut self, ui: &mut egui::Ui) {
        let tags = shown(&self.world, &self.play).all_tags();
        match self.tags.show(ui, &tags) {
            TagAction::Select(tag) => self.select_tagged(&tag),
            TagAction::Rename(old, new) => self.rename_tag(&old, &new),
            TagAction::None => {}
        }
    }

    /// Selects every Instance in the world with `tag`.
    pub(in crate::app) fn select_tagged(&mut self, tag: &str) {
        let tagged = shown(&self.world, &self.play).tagged(tag);
        self.select_many(tagged);
    }

    /// Renames `old` to `new` on every Instance in the world that has it,
    /// keeping its place among their tags. One that already has `new` just
    /// loses `old`. One undo step.
    pub(in crate::app) fn rename_tag(&mut self, old: &str, new: &str) {
        let new = new.trim();
        if new.is_empty() || new == old {
            return;
        }
        let ids = shown(&self.world, &self.play).tagged(old);
        let label = format!("Rename tag {old} to {new}");
        self.history.seal();
        self.annotate(&ids, label.clone(), label, |world, id| {
            let Some(instance) = world.get(id) else {
                return Ok(());
            };
            let mut tags = Vec::new();
            for tag in instance.tags() {
                let tag = if tag == old { new } else { tag.as_str() };
                if !tags.iter().any(|t: &String| t == tag) {
                    tags.push(tag.to_owned());
                }
            }
            let annotations = history::Annotations {
                attributes: instance.attributes().clone(),
                tags,
            };
            history::put_annotations(world, id, &annotations);
            Ok(())
        });
        self.history.seal();
    }
}
