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
        if let Action::KeepPlayChanges(id) = action {
            if let Some(session) = &mut self.play {
                session.kept_mut().keep_whole(id);
                let name = session.world().get(id).map(|i| i.name().to_owned());
                if let Some(name) = name {
                    self.output
                        .info(format!("Stop will keep the changes to {name}."));
                }
            }
            return;
        }
        if self.viewport.is_dragging() || self.playing() {
            return;
        }
        // A script from the `scripts` folder is its file; one inside an
        // object (G2) is an ordinary Instance whose file its copies share.
        let is_script = |id| self.world.get(id).is_some_and(tree::is_file_script);
        // A graph's LocalScript changes only with its graph.
        if let Action::Rename(id, _) | Action::Delete(id) | Action::Duplicate(id) = &action
            && let Some(gui) = self.graph_script_gui(*id)
        {
            self.explain_graph_script(gui);
            return;
        }
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
            // A script that is a file in its service is attached instead:
            // its file moves into `objects/`, which is asked about first.
            Action::MoveInto { id, parent } if is_script(id) => self.ask_attach(id, parent),
            Action::MoveInto { id, parent } => {
                self.move_into(id, parent);
            }
            Action::Animate(id) => self.animate(id),
            Action::ExportPackage(id) => self.export_object_package_with_dialog(id),
            Action::SaveMaterial(id) => self.save_material(id),
            Action::MakeUnique(id) => self.make_unique(id),
            Action::Prefab(action) => self.prefab_action(action),
            Action::KeepPlayChanges(_) => {}
            Action::SetLocked(id, on) => self.set_locked(id, on),
            Action::ReportBug(id) => self.report_bug_about(id),
        }
    }
    /// Turns `Locked` on or off for `id`, and for the rest of the selection
    /// if `id` is in it, as one undo step.
    fn set_locked(&mut self, id: InstanceId, on: bool) {
        let ids = if self.selection.contains(&id) {
            self.selection.clone()
        } else {
            vec![id]
        };
        let edits: Vec<(InstanceId, Vec<Edit>)> = ids
            .into_iter()
            .filter(|&id| {
                self.world
                    .get(id)
                    .is_some_and(|instance| instance.class().can_lock())
            })
            .map(|id| (id, vec![Edit::Property("Locked", Value::Bool(on))]))
            .collect();
        let verb = if on { "Lock" } else { "Unlock" };
        let label = match edits.as_slice() {
            [(only, _)] => {
                let name = self.world.get(*only).map_or("", |i| i.name());
                format!("{verb} {name}")
            }
            many => format!("{verb} {} objects", many.len()),
        };
        self.change_many(Field::Property("Locked"), label, edits);
        self.history.seal();
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
        // A light or emitter with no part to go in stands on its own at the
        // camera's focus point, as in Unreal: no lamp or holder part.
        if what.class().can_stand_free()
            && !tree::light_has_part_parent(&self.world, near.or(self.selected))
        {
            let at = self.insert_position();
            self.insert_named(
                what,
                near.or(self.selected),
                what.class().as_str(),
                vec![("Position", Value::Vector3(at))],
            );
            return;
        }
        if what.class().is_character_controller() {
            let near = near.or(self.selected);
            let parent = tree::insert_parent(&self.world, near, what);
            let has_body = self.world.get(parent).is_some_and(|instance| {
                matches!(
                    instance.class(),
                    ClassName::Model
                        | ClassName::Part
                        | ClassName::MeshPart
                        | ClassName::Seat
                        | ClassName::VehicleSeat
                        | ClassName::DamageZone
                        | ClassName::ReverbZone
                )
            });
            // A character has one controller or the other. One that already
            // has a controller keeps it, and the new one gets a new character.
            let taken = tree::has_controller(&self.world, parent);
            if !has_body || taken {
                let near = if taken { None } else { near };
                self.insert_named(Insertable::Model, near, "Character", Vec::new());
                let model = self.selected;
                self.insert_named(
                    Insertable::Part(PartType::Block),
                    model,
                    "Body",
                    vec![
                        ("Size", Value::Vector3(Vector3::new(2.0, 5.0, 1.0))),
                        ("Color", Value::Color3(Color3::new(0.29, 0.45, 0.78))),
                    ],
                );
                self.insert_named(what, model, what.class().as_str(), Vec::new());
                return;
            }
        }
        // A fog volume or an interior: the volume, then its FogEffect.
        if matches!(what, Insertable::FogVolume | Insertable::InteriorVolume) {
            let name = if what == Insertable::FogVolume {
                "FogVolume"
            } else {
                "Interior"
            };
            self.insert_named(what, None, name, Vec::new());
            let volume = self.selected;
            let fog: Vec<(&'static str, Value)> = if what == Insertable::FogVolume {
                vec![
                    ("Density", Value::Number(0.02)),
                    ("Glare", Value::Number(2.0)),
                    ("HeightFalloff", Value::Number(20.0)),
                    ("Color", Value::Color3(Color3::new(0.85, 0.88, 0.92))),
                ]
            } else {
                vec![
                    ("Interior", Value::Bool(true)),
                    ("Density", Value::Number(0.015)),
                    ("Glare", Value::Number(3.0)),
                    ("Color", Value::Color3(Color3::new(0.95, 0.9, 0.82))),
                ]
            };
            self.insert_named(Insertable::FogEffect, volume, "Fog", fog);
            // The volume stays selected, to size it.
            if let Some(volume) = volume {
                self.select_from_viewport(Some(volume));
            }
            return;
        }
        if what == Insertable::NavigationAgent {
            self.insert_agent(near.or(self.selected));
            return;
        }
        if what == Insertable::Spline {
            self.insert_spline();
            return;
        }
        if matches!(
            what,
            Insertable::SplineMesh | Insertable::SplineScatter | Insertable::TerrainPath
        ) {
            self.insert_spline_generator(what, near.or(self.selected));
            return;
        }
        if what == Insertable::BuildingGenerator {
            self.insert_building(near.or(self.selected));
            return;
        }
        if what == Insertable::SplineFollower {
            let spline = crate::splines::splines(&self.world).first().copied();
            self.insert_named(
                what,
                near,
                what.class().as_str(),
                vec![("Spline", Value::Instance(spline))],
            );
            return;
        }
        // A Health belongs to the Model the selection is, or is in.
        if what == Insertable::Health {
            let model = tree::model_of(&self.world, near.or(self.selected));
            if model.is_none() {
                self.output.warning(
                    "A Health works inside a Model: select a character (or a part of one) \
                     and insert it again, or move this one into a Model.",
                );
            }
            self.insert_named(what, model, what.class().as_str(), Vec::new());
            return;
        }
        // So does an Inventory: the character's.
        if what == Insertable::Inventory {
            let model = tree::model_of(&self.world, near.or(self.selected));
            if model.is_none() {
                self.output.warning(
                    "An Inventory works inside a character's Model: select the character \
                     (or a part of it) and insert it again, or move this one into it.",
                );
            }
            self.insert_named(what, model, what.class().as_str(), Vec::new());
            return;
        }
        // So does a Humanoid; Play makes the pieces its Model lacks.
        if what == Insertable::Humanoid {
            let model = tree::model_of(&self.world, near.or(self.selected));
            if model.is_none() {
                self.output.warning(
                    "A Humanoid works inside a character's Model: select the character \
                     (or a part of it) and insert it again, or move this one into it.",
                );
            }
            self.insert_named(what, model, what.class().as_str(), Vec::new());
            return;
        }
        // The whole character is one undo step; its clips are files.
        if what == Insertable::Character {
            let parent = tree::insert_parent(&self.world, near, what);
            let position = self.insert_position();
            let state_machine = self.character_state_machine();
            let character = tree::character(&mut self.world, position, &state_machine);
            let name = tree::unique_name(&self.world, parent, "Character");
            self.world.set_name(character, &name).expect("just made");
            let index = self.world.get(parent).map_or(0, |p| p.children().len());
            if self.place(
                character,
                Some(Place { parent, index }),
                format!("Insert {name}"),
            ) {
                self.select_from_viewport(Some(character));
            }
            return;
        }
        // The Tool and its Handle are one undo step.
        if what == Insertable::Tool {
            let parent = tree::insert_parent(&self.world, near, what);
            let position = self.insert_position();
            let tool = tree::tool(&mut self.world, position);
            let name = tree::unique_name(&self.world, parent, "Tool");
            self.world.set_name(tool, &name).expect("just made");
            let index = self.world.get(parent).map_or(0, |p| p.children().len());
            if self.place(
                tool,
                Some(Place { parent, index }),
                format!("Insert {name}"),
            ) {
                self.select_from_viewport(Some(tool));
            }
            return;
        }
        if what == Insertable::HealthBar {
            self.insert_health_bar(near.or(self.selected));
            return;
        }
        // The whole car is one undo step.
        if what == Insertable::Vehicle {
            let parent = tree::insert_parent(&self.world, near, what);
            let position = self.insert_position();
            let car = tree::vehicle(&mut self.world, position);
            let name = tree::unique_name(&self.world, parent, "Car");
            self.world.set_name(car, &name).expect("just made");
            let index = self.world.get(parent).map_or(0, |p| p.children().len());
            if self.place(car, Some(Place { parent, index }), format!("Insert {name}")) {
                self.select_from_viewport(Some(car));
            }
            return;
        }
        // A viewpoint is where the editor's camera is, looking the same way.
        if what == Insertable::Camera {
            let (position, orientation) = self.viewport.camera_view();
            self.insert_named(
                what,
                near,
                what.class().as_str(),
                vec![
                    ("Position", Value::Vector3(position)),
                    ("Orientation", Value::Vector3(orientation)),
                ],
            );
            return;
        }
        if what == Insertable::Checkpoint {
            let at = tree::top_of(&self.world, near.or(self.selected)).unwrap_or_default();
            self.insert_named(
                what,
                near,
                what.class().as_str(),
                vec![("Position", Value::Vector3(at))],
            );
            return;
        }
        // An exposure, fog or bloom effect only works in a GraphicsVolume:
        // with none selected, one is made for it.
        if matches!(
            what,
            Insertable::ExposureEffect | Insertable::FogEffect | Insertable::BloomEffect
        ) && !near
            .or(self.selected)
            .is_some_and(|id| tree::is_graphics_volume(&self.world, id))
        {
            self.insert(Insertable::GraphicsVolume, near.or(self.selected));
            let volume = self.selected;
            self.insert_named(what, volume, what.class().as_str(), Vec::new());
            return;
        }
        // A probe works in its part's box: with no part selected, a
        // room-sized invisible one is made for it.
        if matches!(
            what,
            Insertable::ReflectionProbe | Insertable::LightProbeVolume
        ) && !tree::light_has_part_parent(&self.world, near.or(self.selected))
        {
            let name = match what {
                Insertable::ReflectionProbe => "ReflectionProbeBox",
                _ => "LightProbeBox",
            };
            self.insert_named(
                Insertable::Part(PartType::Block),
                near.or(self.selected),
                name,
                vec![
                    ("Anchored", Value::Bool(true)),
                    ("Size", Value::Vector3(Vector3::new(16.0, 10.0, 16.0))),
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
        if what == Insertable::Ragdoll {
            let parent = tree::insert_parent(&self.world, near.or(self.selected), what);
            let in_mesh_part = self
                .world
                .get(parent)
                .is_some_and(|instance| instance.class() == ClassName::MeshPart);
            if !in_mesh_part {
                self.output.warning(
                    "A Ragdoll is built from the skinned MeshPart it's inside: select an imported character first.",
                );
            }
        }
        if what == Insertable::IKControl {
            let parent = tree::insert_parent(&self.world, near.or(self.selected), what);
            let in_mesh_part = self
                .world
                .get(parent)
                .is_some_and(|instance| instance.class() == ClassName::MeshPart);
            if !in_mesh_part {
                self.output.warning(
                    "An IKControl turns the bones of the skinned MeshPart it's inside: select an imported character first.",
                );
            }
        }
        if what == Insertable::Fracture {
            let parent = tree::insert_parent(&self.world, near.or(self.selected), what);
            let in_part = self.world.get(parent).is_some_and(|instance| {
                instance.class().is_base_part() && instance.class() != ClassName::Terrain
            });
            if !in_part {
                self.output.warning(
                    "A Fracture breaks the Part or MeshPart it's inside: select one first.",
                );
            }
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
                    ClassName::Part
                        | ClassName::MeshPart
                        | ClassName::Seat
                        | ClassName::VehicleSeat
                        | ClassName::DamageZone
                        | ClassName::ReverbZone
                        | ClassName::Model
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
        // Parts from the menus start as blueprints to build with.
        let look = match what {
            Insertable::Part(_) => vec![("BlueprintTexture", Value::Bool(true))],
            _ => Vec::new(),
        };
        self.insert_named(what, near, what.class().as_str(), look);
    }
    /// Inserts a NavigationAgent into the Model or part `near` is (or is
    /// in), or into a new Model with a Body when there's none. Its
    /// Destination starts where the body stands, so it stays put in Play
    /// until it's given somewhere to go.
    fn insert_agent(&mut self, near: Option<InstanceId>) {
        let what = Insertable::NavigationAgent;
        let parent = tree::insert_parent(&self.world, near, what);
        let has_body = self.world.get(parent).is_some_and(|instance| {
            matches!(
                instance.class(),
                ClassName::Model
                    | ClassName::Part
                    | ClassName::MeshPart
                    | ClassName::Seat
                    | ClassName::VehicleSeat
                    | ClassName::DamageZone
                    | ClassName::ReverbZone
            )
        });
        let holder = if has_body {
            Some(parent)
        } else {
            self.insert_named(Insertable::Model, near, "Agent", Vec::new());
            let model = self.selected;
            self.insert_named(
                Insertable::Part(PartType::Block),
                model,
                "Body",
                vec![
                    ("Size", Value::Vector3(Vector3::new(2.0, 5.0, 1.0))),
                    ("Color", Value::Color3(Color3::new(0.78, 0.3, 0.29))),
                ],
            );
            model
        };
        let parts = holder.map_or_else(Vec::new, |holder| {
            axcenity_character::body_parts(&self.world, holder)
        });
        let feet = axcenity_nav::feet_of(&self.world, &parts).unwrap_or_default();
        self.insert_named(
            what,
            holder,
            what.class().as_str(),
            vec![("Destination", Value::Vector3(feet))],
        );
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
        let placed = properties
            .iter()
            .any(|(property, _)| *property == "Position");
        for (property, value) in properties {
            self.world
                .set_property(id, property, value)
                .expect("callers pass properties the class has");
        }
        if !placed {
            self.put_in_front_of_camera(id);
        }
        let name = tree::unique_name(&self.world, parent, wanted);
        self.world.set_name(id, &name).expect("just created");
        let end = self.world.get(parent).map_or(0, |p| p.children().len());
        let place = Place { parent, index: end };
        if self.place(id, Some(place), format!("Insert {name}")) {
            self.select_from_viewport(Some(id));
        }
    }
    /// Moves the new part `id` (not yet in the world's tree) in front of
    /// the camera: resting on what the middle of the view looks at, on the
    /// move grid, and lifted out of any solid part already there
    /// (`spawning`). Anything that isn't a part is left where it is.
    fn put_in_front_of_camera(&mut self, id: InstanceId) {
        let Some(instance) = self.world.get(id) else {
            return;
        };
        if !instance.class().is_base_part() || instance.class() == ClassName::Terrain {
            return;
        }
        let Some(size) = instance.property("Size").and_then(Value::as_vector3) else {
            return;
        };
        let spot = self.viewport.spawn_spot(&self.world);
        // Rounded to 3 decimals so the numbers stay tidy; `+ 0.0` turns -0 into 0.
        let tidy = |value: f32| (self.snap.studs(value) * 1000.0).round() / 1000.0 + 0.0;
        let size = axcenity_render::glam::Vec3::new(size.x, size.y, size.z);
        let at = crate::spawning::center(spot, size, tidy);
        let _ = self.world.set_property(
            id,
            "Position",
            Value::Vector3(Vector3::new(at.x, at.y, at.z)),
        );
        if !spot.floating {
            crate::spawning::lift_clear(&mut self.world, id, spot.normal);
        }
    }

    /// The state machine Insert > Character's Animator plays:
    /// `assets/animations/Character/Character.statemachine.json`, made the
    /// first time with an empty clip for each of [`tree::CHARACTER_STATES`].
    /// Empty (and a warning) when there's no project folder to write to.
    fn character_state_machine(&mut self) -> String {
        let Some(project) = self.project.as_ref().map(|project| project.folder.clone()) else {
            self.output.warning(
                "The Character's Animator has no state machine: save the project first, \
                 then insert another to get its Idle, Walk, Run, Jump, Fall and Sit clips.",
            );
            return String::new();
        };
        let path = format!(
            "{}/Character/Character{}",
            axcenity_anim::ANIMATIONS_FOLDER,
            axcenity_anim::STATE_MACHINE_EXTENSION
        );
        if project.join(&path).is_file() {
            return path;
        }
        let clips: Vec<(String, axcenity_anim::ClipFile)> = tree::CHARACTER_STATES
            .iter()
            .map(|state| ((*state).to_owned(), axcenity_anim::ClipFile::new(1.0)))
            .collect();
        match crate::anim_import::write_animation(&project, "Character", &clips, true) {
            Ok((_, Some(controller))) => {
                self.output.info(format!(
                    "Made the state machine `{controller}`, with an empty clip for each \
                     state to replace with the character's own."
                ));
                controller
            }
            Ok((_, None)) => String::new(),
            Err(err) => {
                self.output
                    .error(format!("Could not write the Character's clips: {err}"));
                String::new()
            }
        }
    }

    /// Insert > UI > Health bar: a billboard over the nearest Health's
    /// character and a HUD bar, both bound to that Health, styled by
    /// `health-bar.css` (made if no stylesheet has `.health-bar`). The
    /// Instances and the stylesheet are one undo step.
    fn insert_health_bar(&mut self, near: Option<InstanceId>) {
        let position = self.insert_position();
        let health = tree::nearest_health(&self.world, near, position);
        let made = tree::health_bar(&mut self.world, health);
        let mut moves = Vec::new();
        let mut put = |world: &mut World, id: InstanceId, parent: InstanceId| {
            let name = world
                .get(id)
                .map(|i| i.name().to_owned())
                .unwrap_or_default();
            let name = tree::unique_name(world, parent, &name);
            world.set_name(id, &name).expect("just made");
            let index = world.get(parent).map_or(0, |p| p.children().len());
            let place = Place { parent, index };
            if history::put(world, id, Some(place)).is_ok() {
                moves.push(Move {
                    id,
                    before: None,
                    after: Some(place),
                });
            }
        };
        if let Some((billboard, part)) = made.billboard {
            put(&mut self.world, billboard, part);
        }
        let workspace = self.world.workspace();
        put(&mut self.world, made.hud, workspace);
        let texts = self.health_bar_styles().into_iter().collect();
        self.history.record_files_and_tree(
            &mut self.world,
            "Insert Health bar".to_owned(),
            texts,
            moves,
        );
        self.history.seal();
        match health {
            Some(health) => {
                let name = self.world.get(health).and_then(|h| h.parent());
                let name = name
                    .and_then(|model| self.world.get(model))
                    .map_or("its Model", |model| model.name())
                    .to_owned();
                self.output.info(format!(
                    "The health bars follow the Health in {name}. To follow another, set the \
                     Value of the ObjectValue named Health inside each bar."
                ));
            }
            None => self.output.warning(
                "There's no Health in the Workspace, so the HUD bar follows nothing and no \
                 billboard was made. Insert a Health into a character, then set the Value of \
                 the ObjectValue named Health inside the bar.",
            ),
        }
        self.select_from_viewport(Some(made.hud));
    }
    /// `health-bar.css` with the health bars' styles, made unless a
    /// stylesheet already has a `.health-bar` rule. `None` when nothing was
    /// made (or there's no project to make it in).
    fn health_bar_styles(&mut self) -> Option<history::TextChange> {
        let project = self.project.as_ref()?;
        let selector = format!(".{}", tree::HEALTH_BAR_CLASS);
        let styled = project
            .stylesheets
            .iter()
            .any(|sheet| sheet.rule_end(&selector).is_some());
        if styled {
            return None;
        }
        let folder = project.folder.clone();
        let source = tree::HEALTH_BAR_CSS;
        match axcenity_project::create_stylesheet(&folder, tree::HEALTH_BAR_CLASS, source) {
            Ok(path) => {
                if let Some(project) = &mut self.project {
                    project
                        .stylesheet_texts
                        .insert(path.clone(), source.to_owned());
                }
                self.refresh_stylesheet(&path, source);
                self.tabs.open_in_background(&path, source);
                Some(history::TextChange {
                    path,
                    before: None,
                    after: Some(source.to_owned()),
                })
            }
            Err(err) => {
                self.output
                    .error(format!("Could not make the health bar's stylesheet: {err}"));
                None
            }
        }
    }
    /// The Weld button: welds the selected parts (and the parts inside
    /// selected Models) to the largest of them where they are, with a
    /// WeldConstraint inside each other part, as one undo step.
    pub(super) fn weld_selected(&mut self) {
        if self.playing() || self.viewport.is_dragging() {
            return;
        }
        let pairs = crate::constraints::weld_pairs(&self.world, &self.tool_parts());
        if pairs.is_empty() {
            self.output.warning("Select two or more parts to weld.");
            return;
        }
        let label = format!("Weld {} parts", pairs.len() + 1);
        self.add_welds(pairs, label);
    }
    /// Join Surfaces: welds the parts a drag just moved to the parts
    /// they're left touching, as one undo step after the drag's.
    pub(super) fn join_surfaces(&mut self, moved: &[InstanceId]) {
        if self.playing() {
            return;
        }
        let pairs = crate::snapping::joins(&self.world, moved);
        if pairs.is_empty() {
            return;
        }
        let label = match pairs.len() {
            1 => "Join 1 surface".to_owned(),
            n => format!("Join {n} surfaces"),
        };
        self.add_welds(pairs, label);
    }
    /// A WeldConstraint for each `(Part0, Part1)` pair, inside Part1, as
    /// one undo step named `label`.
    fn add_welds(&mut self, pairs: Vec<(InstanceId, InstanceId)>, label: String) {
        let mut moves = Vec::new();
        for (part0, part1) in pairs {
            let weld = crate::constraints::new_weld(&mut self.world, part0, part1);
            let name = tree::unique_name(&self.world, part1, ClassName::WeldConstraint.as_str());
            self.world.set_name(weld, name).expect("just made");
            let index = self.world.get(part1).map_or(0, |p| p.children().len());
            let place = Place {
                parent: part1,
                index,
            };
            if history::put(&mut self.world, weld, Some(place)).is_ok() {
                moves.push(Move {
                    id: weld,
                    before: None,
                    after: Some(place),
                });
            }
        }
        self.history.record_tree(&mut self.world, label, moves);
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
    /// Duplicates every selected Instance that isn't inside another one, as
    /// one undo step, and selects the copies. Each copy goes right after its
    /// original. A UI element in a copy gets a fresh `Id`, with copies of
    /// its original's own rules (`#Id`, `#Id:hover` and so on).
    pub(super) fn duplicate_selected(&mut self) {
        let ids = self.selection_roots();
        let mut copies = Vec::new();
        let mut moves = Vec::new();
        let mut names = Vec::new();
        // Backwards, so each original's place is still right when its copy goes in.
        for id in ids.into_iter().rev() {
            let Some(name) = self.editable_name(id, "duplicate") else {
                continue;
            };
            let Some(original) = history::place_of(&self.world, id) else {
                continue;
            };
            let copy = self.world.duplicate(id).expect("checked it can be edited");
            let copy_name = tree::unique_name(&self.world, original.parent, &name);
            self.world.set_name(copy, copy_name).expect("just made");
            let after = Place {
                parent: original.parent,
                index: original.index + 1,
            };
            if let Err(err) = history::put(&mut self.world, copy, Some(after)) {
                self.output.error(format!("Could not do that: {err}"));
                let _ = self.world.destroy(copy);
                continue;
            }
            moves.push(Move {
                id: copy,
                before: None,
                after: Some(after),
            });
            copies.push(copy);
            names.push(name);
        }
        if copies.is_empty() {
            return;
        }
        let texts = self.reid_duplicates(&copies);
        let label = match names.as_slice() {
            [name] => format!("Duplicate {name}"),
            _ => format!("Duplicate {} items", names.len()),
        };
        self.history
            .record_files_and_tree(&mut self.world, label, texts, moves);
        self.history.seal();
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
        if ids.len() == 1 && self.world.get(ids[0]).is_some_and(tree::is_file_script) {
            self.confirm_delete = Some(ids[0]);
            return;
        }
        let mut skipped_script = false;
        for id in ids {
            if self.world.get(id).is_some_and(tree::is_file_script) {
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
        // A light or emitter taken out of its part stays where it was
        // shining from, instead of jumping to its old Position.
        let stays = self.free_placement_after_move(id, parent);
        let moved = match stays {
            Some(edits) => self.place_keeping(id, place, label, edits),
            None => self.place(id, Some(place), label),
        };
        if moved {
            // Opens the new parent, so the moved row stays in view.
            self.explorer.reveal(&self.world, id);
        }
        moved
    }
    /// For a light or ParticleEmitter in a part about to go into `parent`,
    /// which isn't a part: its Position and Orientation set to where it
    /// works from now, so it stands free right there.
    fn free_placement_after_move(
        &self,
        id: InstanceId,
        parent: InstanceId,
    ) -> Option<Vec<(&'static str, Value)>> {
        let instance = self.world.get(id)?;
        if !instance.class().can_stand_free()
            || self
                .world
                .get(parent)
                .is_some_and(|parent| parent.class().is_base_part())
        {
            return None;
        }
        let marker = axcenity_scene::light_markers(&self.world)
            .into_iter()
            .find(|marker| marker.id == id && marker.part.is_some())?;
        let rotation = match axcenity_scene::mount_of(&self.world, instance)? {
            axcenity_scene::Mount::Part { transform, .. } => {
                transform.to_scale_rotation_translation().1
            }
            axcenity_scene::Mount::Free { .. } => return None,
        };
        let tidy = |value: f32| (value * 1000.0).round() / 1000.0 + 0.0;
        let at = marker.position;
        let (y, x, z) = rotation.to_euler(axcenity_render::glam::EulerRot::YXZ);
        Some(vec![
            (
                "Position",
                Value::Vector3(Vector3::new(tidy(at.x), tidy(at.y), tidy(at.z))),
            ),
            (
                "Orientation",
                Value::Vector3(Vector3::new(
                    tidy(x.to_degrees()),
                    tidy(y.to_degrees()),
                    tidy(z.to_degrees()),
                )),
            ),
        ])
    }
    /// [`Self::place`] that also sets `values` on `id`, in the same step.
    fn place_keeping(
        &mut self,
        id: InstanceId,
        place: Place,
        label: String,
        values: Vec<(&'static str, Value)>,
    ) -> bool {
        let before = history::place_of(&self.world, id);
        if let Err(err) = history::put(&mut self.world, id, Some(place)) {
            self.output.error(format!("Could not do that: {err}"));
            return false;
        }
        let mut properties = Vec::new();
        for (name, value) in values {
            let Some(old) = self.world.get(id).and_then(|i| i.property(name)).cloned() else {
                continue;
            };
            if self.world.set_property(id, name, value.clone()).is_ok() {
                properties.push(history::PropertyEdit {
                    id,
                    field: Field::Property(name),
                    before: old,
                    after: value,
                });
            }
        }
        let moves = vec![Move {
            id,
            before,
            after: Some(place),
        }];
        self.history
            .record_tree_and_properties(&mut self.world, label, moves, properties);
        true
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
