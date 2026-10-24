//! Insert, duplicate, delete and reparent Instances.
//!
//! Tree edits are undo steps. A script is deleted through `files.rs` so its
//! file goes with it. These are methods of [`super::EditorApp`].

use super::*;

impl EditorApp {
    /// Does something to the tree from the Explorer, the toolbar or a
    /// shortcut. Ignored while a part is being dragged in the viewport, and
    /// during Play.
    pub(super) fn act(&mut self, action: Action) {
        if self.viewport.is_dragging() || self.playing() {
            return;
        }
        let is_script = |id| {
            self.world
                .get(id)
                .is_some_and(|instance| instance.class().is_script())
        };
        match action {
            Action::Insert { what, near } => self.insert(what, near),
            Action::InsertScript(class) => self.insert_script(class),
            // A script's name is its file's name, so the file is renamed.
            Action::Rename(id, name) if is_script(id) => self.rename_script(id, &name),
            Action::Rename(id, name) => {
                self.apply(id, Edit::Name(name));
                // Typing in the Explorer is done, so this is one whole step.
                self.history.seal();
            }
            Action::Duplicate(id) => {
                if self.selection.contains(&id) && self.selection.len() > 1 {
                    self.duplicate_selected();
                } else {
                    self.duplicate(id);
                }
            }
            // Deleting a script deletes its file, which can't be undone, so ask first.
            Action::Delete(id) if is_script(id) => self.confirm_delete = Some(id),
            Action::Delete(id) if self.selection.contains(&id) && self.selection.len() > 1 => {
                self.delete_selected();
            }
            Action::Delete(id) => self.delete(id),
            Action::MoveInto { id, parent } => {
                self.move_into(id, parent);
            }
            Action::Animate(id) => self.animate(id),
        }
    }
    /// Where a new part goes: the camera's focus point, on the move grid
    /// when snapping is on, so new parts line up.
    pub(super) fn insert_position(&self) -> Vector3 {
        let focus = self.viewport.focus();
        // Rounded to 3 decimals so the numbers stay tidy; `+ 0.0` turns -0 into 0.
        let tidy = |value: f32| (self.snap.studs(value) * 1000.0).round() / 1000.0 + 0.0;
        Vector3::new(tidy(focus.x), tidy(focus.y), tidy(focus.z))
    }
    /// Inserts `what` where [`tree::insert_parent`] says, with a name no
    /// sibling has, and selects it. A part goes at the camera's focus point.
    ///
    /// UI elements only show inside a ScreenGui, so inserting one where
    /// there's none makes a ScreenGui for it first.
    pub(super) fn insert(&mut self, what: Insertable, near: Option<InstanceId>) {
        if matches!(what, Insertable::PointLight | Insertable::SpotLight)
            && !tree::light_has_part_parent(&self.world, near.or(self.selected))
        {
            self.insert_named(
                Insertable::Part(PartType::Ball),
                near.or(self.selected),
                "Lamp",
                vec![
                    ("Anchored", Value::Bool(true)),
                    ("Material", Value::Material(Material::Neon)),
                    ("Size", Value::Vector3(Vector3::new(1.2, 1.2, 1.2))),
                    ("Color", Value::Color3(Color3::new(1.0, 0.86, 0.55))),
                ],
            );
            let lamp = self.selected;
            self.insert_named(what, lamp, what.class().as_str(), Vec::new());
            return;
        }
        if what == Insertable::ParticleEmitter
            && !tree::light_has_part_parent(&self.world, near.or(self.selected))
        {
            self.insert_named(
                Insertable::Part(PartType::Block),
                near.or(self.selected),
                "Emitter",
                vec![
                    ("Anchored", Value::Bool(true)),
                    ("Size", Value::Vector3(Vector3::new(1.0, 1.0, 1.0))),
                    ("Transparency", Value::Number(1.0)),
                    ("CanCollide", Value::Bool(false)),
                    ("CastShadow", Value::Bool(false)),
                ],
            );
            let holder = self.selected;
            self.insert_named(what, holder, what.class().as_str(), Vec::new());
            return;
        }
        if what == Insertable::Page {
            let layout =
                tree::page_layout_for(&self.world, near.or(self.selected)).unwrap_or_else(|| {
                    self.insert(Insertable::PageLayout, near);
                    self.selected.expect("inserting a PageLayout selects it")
                });
            self.insert_named(Insertable::Frame, Some(layout), "Page", Vec::new());
            return;
        }
        if what == Insertable::BillboardGui
            && !tree::light_has_part_parent(&self.world, near.or(self.selected))
        {
            self.output
                .warning("BillboardGui shows when it is inside a part.");
        }
        if what == Insertable::Animator {
            let parent = tree::insert_parent(&self.world, near.or(self.selected), what);
            let in_mesh_part = self
                .world
                .get(parent)
                .is_some_and(|instance| instance.class() == ClassName::MeshPart);
            if !in_mesh_part {
                self.output.warning(
                    "An Animator animates the MeshPart it's inside: select an imported animated model first.",
                );
            }
        }
        if what == Insertable::Animation {
            let parent = tree::insert_parent(&self.world, near.or(self.selected), what);
            let ok = self.world.get(parent).is_some_and(|instance| {
                matches!(
                    instance.class(),
                    ClassName::Part | ClassName::MeshPart | ClassName::Model
                )
            });
            if !ok {
                self.output.warning(
                    "An Animation plays a property clip on the Part or Model it's inside.",
                );
            }
        }
        if tree::needs_screen_gui(&self.world, near, what) {
            self.insert_named(Insertable::ScreenGui, None, "ScreenGui", Vec::new());
            let gui = self.selected;
            self.insert_named(what, gui, what.class().as_str(), Vec::new());
            return;
        }
        self.insert_named(what, near, what.class().as_str(), Vec::new());
    }
    /// Like [`insert`](Self::insert), but named `wanted` (with a number on
    /// the end if a sibling has that name) and with `properties` set.
    pub(super) fn insert_named(
        &mut self,
        what: Insertable,
        near: Option<InstanceId>,
        wanted: &str,
        properties: Vec<(&'static str, Value)>,
    ) {
        let parent = tree::insert_parent(&self.world, near, what);
        // Worked out first: `create` borrows the world mutably, and
        // `insert_position` needs to borrow `self` while that lasts.
        let position = self.insert_position();
        let id = tree::create(&mut self.world, what, position);
        for (property, value) in properties {
            self.world
                .set_property(id, property, value)
                .expect("callers pass properties the class has");
        }
        let name = tree::unique_name(&self.world, parent, wanted);
        self.world.set_name(id, &name).expect("just created");
        let end = self.world.get(parent).map_or(0, |p| p.children().len());
        let place = Place { parent, index: end };
        if self.place(id, Some(place), format!("Insert {name}")) {
            self.select_from_viewport(Some(id));
        }
    }
    /// Takes `id` and everything inside it out of the world, as one undo step.
    pub(super) fn delete(&mut self, id: InstanceId) {
        let Some(name) = self.editable_name(id, "delete") else {
            return;
        };
        if self.place(id, None, format!("Delete {name}")) {
            self.forget_selection_outside_tree();
        }
    }
    /// Copies `id` and everything inside it, puts the copy right after it
    /// with a name no sibling has, and selects the copy.
    pub(super) fn duplicate(&mut self, id: InstanceId) -> Option<InstanceId> {
        let name = self.editable_name(id, "duplicate")?;
        let original = history::place_of(&self.world, id)?;
        let copy = self.world.duplicate(id).expect("checked it can be edited");
        let copy_name = tree::unique_name(&self.world, original.parent, &name);
        self.world.set_name(copy, copy_name).expect("just made");
        let after = Place {
            parent: original.parent,
            index: original.index + 1,
        };
        if self.place(copy, Some(after), format!("Duplicate {name}")) {
            self.select_from_viewport(Some(copy));
            return Some(copy);
        }
        None
    }
    /// Duplicates every selected Instance that isn't inside another one, and
    /// selects the copies.
    pub(super) fn duplicate_selected(&mut self) {
        let ids = self.selection_roots();
        let mut copies = Vec::new();
        for id in ids.into_iter().rev() {
            if let Some(copy) = self.duplicate(id) {
                copies.push(copy);
            }
        }
        if copies.is_empty() {
            return;
        }
        copies.reverse();
        let primary = *copies.last().unwrap();
        self.selection = copies;
        self.selected = Some(primary);
        self.explorer.reveal(&self.world, primary);
    }
    /// Deletes every selected Instance that isn't inside another one. One
    /// script still goes through the "delete its file?" question. Several
    /// scripts are left, and a note says to delete those one at a time.
    pub(super) fn delete_selected(&mut self) {
        let ids = self.selection_roots();
        if ids.len() == 1
            && self
                .world
                .get(ids[0])
                .is_some_and(|instance| instance.class().is_script())
        {
            self.confirm_delete = Some(ids[0]);
            return;
        }
        let mut skipped_script = false;
        for id in ids {
            if self
                .world
                .get(id)
                .is_some_and(|instance| instance.class().is_script())
            {
                skipped_script = true;
                continue;
            }
            self.delete(id);
        }
        if skipped_script {
            self.output.info(
                "Scripts are deleted one at a time, from their row's menu, so the file can be removed too.",
            );
        }
    }
    /// Moves `id` into `parent` as its last child, as one undo step. Moves
    /// into itself, into something inside it, or out of the Workspace are
    /// rejected with the reason in Output. Returns true if it moved.
    pub(super) fn move_into(&mut self, id: InstanceId, parent: InstanceId) -> bool {
        let (Some(instance), Some(target)) = (self.world.get(id), self.world.get(parent)) else {
            return false;
        };
        let (name, target_name) = (instance.name().to_string(), target.name().to_string());
        if let Err(reason) = tree::check_move(&self.world, id, parent) {
            self.output
                .error(format!("Can't move {name} into {target_name}: {reason}"));
            return false;
        }
        if instance.parent() == Some(parent) {
            return false;
        }
        let place = Place {
            parent,
            index: target.children().len(),
        };
        let label = format!("Move {name} into {target_name}");
        let moved = self.place(id, Some(place), label);
        if moved {
            // Opens the new parent, so the moved row stays in view.
            self.explorer.reveal(&self.world, id);
        }
        moved
    }
    /// `id`'s name, if it can be deleted, duplicated or moved. Otherwise
    /// says why not in Output (`doing` is "delete", say) and returns `None`.
    pub(super) fn editable_name(&mut self, id: InstanceId, doing: &str) -> Option<String> {
        let name = self.world.get(id)?.name().to_string();
        match tree::check_editable(&self.world, id) {
            Ok(()) => Some(name),
            Err(reason) => {
                self.output.error(format!("Can't {doing} {name}: {reason}"));
                None
            }
        }
    }
    /// Puts `id` at `place` in the tree (`None` takes it out), and records
    /// that as an undo step named `label`. Returns false, with the reason in
    /// Output, if the tree said no.
    pub(super) fn place(&mut self, id: InstanceId, place: Option<Place>, label: String) -> bool {
        let before = history::place_of(&self.world, id);
        if let Err(err) = history::put(&mut self.world, id, place) {
            self.output.error(format!("Could not do that: {err}"));
            return false;
        }
        let moves = vec![Move {
            id,
            before,
            after: place,
        }];
        self.history.record_tree(&mut self.world, label, moves);
        true
    }
}
