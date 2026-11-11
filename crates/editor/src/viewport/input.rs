use super::*;

/// The nearest part along `ray` that isn't locked: a click or a drag goes
/// straight through locked parts.
pub(super) fn pick_unlocked(
    world: &World,
    ray: axcenity_scene::Ray,
) -> Option<axcenity_scene::Hit> {
    let locked: Vec<InstanceId> = world
        .descendants(world.workspace())
        .into_iter()
        .filter(|&id| world.is_locked(id))
        .collect();
    axcenity_scene::pick_hit_skipping(world, ray, &locked)
}

impl Viewport {
    /// The mouse and left button over the terrain `brush` works on.
    pub(super) fn terrain_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        screen: &Screen,
        world: &World,
        brush: TerrainBrush,
    ) -> TerrainPointer {
        let (pressed, down, pointer, modifiers, seconds) = ui.input(|input| {
            (
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
                input.pointer.latest_pos(),
                input.modifiers,
                input.stable_dt.min(MAX_FRAME_SECONDS),
            )
        });
        let over = pointer.filter(|_| response.contains_pointer());
        let point =
            over.and_then(|pointer| ground_point(world, brush.terrain, screen.ray(pointer)));
        // Alt+left-drag orbits the camera instead.
        let pressed = pressed && over.is_some() && !modifiers.alt;
        if pressed {
            self.terrain_stroke = true;
        }
        let released = self.terrain_stroke && !down;
        if released {
            self.terrain_stroke = false;
        }
        if over.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        // Held still, a stroke keeps working (raising, smoothing), so keep
        // drawing frames.
        if self.terrain_stroke {
            ui.ctx().request_repaint();
        }
        TerrainPointer {
            point,
            pressed,
            down: self.terrain_stroke,
            released,
            modifiers,
            seconds,
        }
    }

    /// The mouse over the viewport for the editor's plugins: what it points
    /// at, and whether it moved or its left button went down or up.
    /// `None` while it's elsewhere and nothing happened.
    pub(super) fn plugin_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        screen: &Screen,
        world: &World,
    ) -> Option<axcenity_plugins::MouseInput> {
        let (pointer, pressed, released, delta, alt) = ui.input(|input| {
            (
                input.pointer.latest_pos(),
                input.pointer.primary_pressed(),
                input.pointer.primary_released(),
                input.pointer.delta(),
                input.modifiers.alt,
            )
        });
        let over = pointer.filter(|_| response.contains_pointer())?;
        // Alt+left-drag orbits the camera instead.
        let pressed = pressed && !alt;
        if pressed {
            self.plugin_press = true;
        }
        let released = released && std::mem::take(&mut self.plugin_press);
        let moved = delta != egui::Vec2::ZERO;
        if !(pressed || released || moved) {
            return None;
        }
        let ray = screen.ray(over);
        let hit = axcenity_scene::pick_hit(world, ray, None);
        // Nothing under the mouse: a point far along the ray.
        let distance = hit.map_or(1000.0, |hit| hit.distance);
        let point = ray.origin + ray.direction * distance;
        Some(axcenity_plugins::MouseInput {
            state: axcenity_plugins::MouseState {
                hit: axcenity_core::Vector3::new(point.x, point.y, point.z),
                target: hit.map(|hit| hit.part),
            },
            moved,
            pressed,
            released,
        })
    }

    /// The Constraint tool: marks the part surface under the mouse (and
    /// `first`, the surface already picked), and gives the one clicked.
    /// Alt+left-drag still orbits the camera.
    pub(super) fn surface_input(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
        screen: &Screen,
        world: &World,
        (first, points_on): (Option<Vec3>, bool),
    ) -> Option<SurfacePick> {
        let (pointer, alt) = ui.input(|input| (input.pointer.latest_pos(), input.modifiers.alt));
        let hover = pointer
            .filter(|_| response.contains_pointer())
            .and_then(|pointer| {
                let ray = screen.ray(pointer);
                let hit = axcenity_scene::pick_hit(world, ray, None)?;
                let class = world.get(hit.part)?.class();
                class.is_base_part().then(|| SurfacePick {
                    part: hit.part,
                    point: ray.at(hit.distance),
                    normal: hit.normal,
                })
            });
        let painter = ui.painter_at(rect);
        // With snap points on, a click near one lands exactly on it.
        let box_under = hover
            .filter(|_| points_on)
            .and_then(|pick| Some(snapping::Obb::of(Pose::of(world.get(pick.part)?)?)));
        let hover = hover.map(|mut pick| {
            if let Some(obb) = box_under {
                let reach = screen.studs_per_point(pick.point) * snapping::SNAP_POINTS;
                pick.point = snapping::points::pull_to_point(&obb, pick.point, reach);
            }
            pick
        });
        if let Some(obb) = box_under {
            super::move_overlay::paint_part_points(&painter, screen, rect, &obb.snap_points());
        }
        let stroke = egui::Stroke::new(SELECTION_WIDTH, SELECTION_COLOR);
        for point in first.into_iter().chain(hover.map(|pick| pick.point)) {
            if let Some(at) = screen.point(point) {
                painter.circle_stroke(at, 7.0, stroke);
                painter.circle_filled(at, 2.5, SELECTION_COLOR);
            }
        }
        if let (Some(first), Some(hover)) = (first, hover)
            && let Some(line) = screen.line(first, hover.point)
        {
            painter.line_segment(line, egui::Stroke::new(1.5_f32, SELECTION_COLOR));
        }
        if hover.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        (response.clicked_by(egui::PointerButton::Primary) && !alt)
            .then_some(hover)
            .flatten()
    }

    /// A press on a Spline's curve or on a point of `editing`, and the
    /// drag of a point after it. Returns what to do, whether the left
    /// button belongs to the Spline this frame (so nothing else takes it),
    /// and whether a point's drag was let go.
    pub(super) fn spline_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        screen: &Screen,
        world: &World,
        editing: Option<InstanceId>,
    ) -> (Option<SplineAction>, bool, bool) {
        let (pressed, down, pointer, origin, alt) = ui.input(|input| {
            (
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
                input.pointer.latest_pos(),
                input.pointer.press_origin(),
                input.modifiers.alt,
            )
        });
        if let Some(drag) = self.spline_drag.as_mut() {
            if down {
                let moved = match (pointer, origin) {
                    (Some(pointer), Some(origin)) => pointer.distance(origin) > 3.0,
                    _ => false,
                };
                let hit = pointer.filter(|_| moved).and_then(|pointer| {
                    let ray = screen.ray(pointer);
                    axcenity_scene::pick_hit(world, ray, None).map(|hit| ray.at(hit.distance))
                });
                if let Some(at) = hit {
                    drag.at = Some(at);
                    if !drag.add {
                        let point = drag.point;
                        return (Some(SplineAction::Move { point, to: at }), true, false);
                    }
                }
                return (None, true, false);
            }
            let drag = *drag;
            self.spline_drag = None;
            let added = match (drag.add, drag.at) {
                (true, Some(at)) => Some(SplineAction::Add {
                    after: drag.point,
                    at,
                }),
                _ => None,
            };
            return (added, true, true);
        }
        if self.spline_press {
            // Pressed on a curve: held until it's let go.
            if !down {
                self.spline_press = false;
            }
            return (None, true, false);
        }
        if !pressed || !response.contains_pointer() {
            return (None, false, false);
        }
        let Some(pointer) = pointer else {
            return (None, false, false);
        };
        if let Some(editing) = editing
            && let Some(point) = crate::splines::hit_point(screen, world, editing, pointer)
        {
            self.spline_drag = Some(SplineDrag {
                point,
                add: alt,
                at: None,
            });
            return (Some(SplineAction::Pick(point)), true, false);
        }
        // Alt+left-drag anywhere else orbits the camera.
        if alt {
            return (None, false, false);
        }
        match crate::splines::hit_curve(screen, world, pointer) {
            Some(spline) => {
                self.spline_press = true;
                (Some(SplineAction::Edit(spline)), true, false)
            }
            None => (None, false, false),
        }
    }

    /// Where an asset dragged from another panel would land, if one is
    /// being dragged and the mouse is over the viewport.
    pub(super) fn drop_target(
        &self,
        ui: &egui::Ui,
        rect: egui::Rect,
        screen: &Screen,
        world: &World,
        ui_scene: &UiScene,
    ) -> Option<DropTarget> {
        if !egui::DragAndDrop::has_any_payload(ui.ctx()) {
            return None;
        }
        let (pointer, released) =
            ui.input(|input| (input.pointer.latest_pos(), input.pointer.any_released()));
        let pointer = pointer.filter(|pointer| rect.contains(*pointer))?;
        let ray = screen.ray(pointer);
        let hit = axcenity_scene::pick_hit(world, ray, None);
        let (point, normal) = match &hit {
            Some(hit) => (ray.at(hit.distance), hit.normal),
            // The ground, if the mouse points down at it, near enough.
            None if ray.direction.y < -0.01 => {
                let distance = (-ray.origin.y / ray.direction.y).min(1000.0);
                (ray.at(distance), Vec3::Y)
            }
            None => (self.camera.focus(), Vec3::Y),
        };
        Some(DropTarget {
            point,
            normal,
            part: hit.map(|hit| hit.part),
            ui: ui_scene.hit(pointer),
            released,
        })
    }

    /// Starts, continues or ends dragging a handle of the selected part or
    /// UI element, or any part or UI element by its body.
    #[allow(clippy::too_many_arguments)] // Each is a different thing dragging needs.
    pub(super) fn drag_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        screen: &Screen,
        world: &World,
        chosen: DragSelection,
        tool: Option<Tool>,
        snap: Snap,
        ui_scene: &UiScene,
        stylesheets: &[Stylesheet],
        lights: &[(LightMarker, egui::Pos2)],
    ) -> DragInput {
        let Some(tool) = tool else {
            // Nothing to drag. A drag can't be under way either: Play
            // doesn't start during one.
            self.drag = None;
            return DragInput::default();
        };
        let DragSelection {
            selected,
            selection,
            modes,
        } = chosen;
        // The handles act on every selected part (and the parts inside
        // selected Models), with the last selected one last, and on lights
        // and emitters standing free.
        let parts = movable_parts(world, selection, selected);
        let poses: Vec<Pose> = parts.iter().map(|(_, pose)| *pose).collect();
        let shapes = if tool.has_handles() {
            gizmo::shapes_for(tool, modes, &poses, screen)
        } else {
            Vec::new()
        };
        let (pressed, down, pointer, press_origin, alt, shift) = ui.input(|input| {
            (
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
                input.pointer.latest_pos(),
                input.pointer.press_origin(),
                input.modifiers.alt,
                input.modifiers.shift,
            )
        });
        let over = response.contains_pointer() && self.drag.is_none();
        let hovered = match pointer {
            Some(pointer) if over => gizmo::hovered(&shapes, pointer),
            _ => None,
        };
        // The UI element's handle under the mouse (not its body: that's
        // only grabbed by dragging, so a click can select what's inside it).
        let ui_hovered = match (pointer, ui_scene.handles) {
            (Some(pointer), Some(handles)) if over => handles.grip_at(pointer),
            _ => None,
        };
        let ui_handle = ui_hovered.filter(|&grip| grip != Grip::BODY);
        let mut input = DragInput::default();

        // Alt+left-drag orbits the camera, so it never grabs anything.
        // A handle is grabbed as soon as the button goes down on it.
        if pressed
            && !alt
            && let (Some(grip), Some(pointer), Some(handles)) =
                (ui_handle, pointer, ui_scene.handles)
        {
            self.drag = UiDrag::start(handles, grip, pointer, world)
                .map(|drag| Dragging::Ui(Box::new(drag)));
        } else if pressed
            && !alt
            && let (Some(handle), Some(pointer)) = (hovered, pointer)
            && let Some(frame) = gizmo::frame_of(&shapes, handle)
        {
            // The other parts, for Surfaces and Collisions.
            let moving: Vec<InstanceId> = parts.iter().map(|(id, _)| *id).collect();
            let reach = screen.studs_per_point(frame.position) * snapping::SNAP_POINTS;
            let obstacles = snapping::obstacles(world, &moving);
            let sites = snapping::points::sites(world, &moving);
            self.drag = Drag::start_many(handle, frame, parts.clone(), modes, screen, pointer)
                .map(|drag| Dragging::Handle(drag.among(obstacles, reach).with_sites(sites)));
        }
        // A UI element or part's body only once the mouse has moved far
        // enough to be a drag, so a plain click still just selects. It's
        // grabbed where the button went down. UI is on top, so it goes first.
        let body_drag =
            self.drag.is_none() && !alt && response.drag_started_by(egui::PointerButton::Primary);
        if body_drag
            && let Some(origin) = press_origin
            && let Some(id) = ui_scene.hit(origin)
        {
            // Selected even when it can't move (in a flex container).
            input.grabbed = Some(id);
            if let Some(&element) = ui_scene.layout.element(id) {
                let handles = Handles::new(
                    element,
                    world,
                    stylesheets,
                    ui_scene.origin,
                    ui_scene.pixels_per_point,
                );
                self.drag = UiDrag::start(&handles, Grip::BODY, origin, world)
                    .map(|drag| Dragging::Ui(Box::new(drag)));
            }
        } else if body_drag
            && let Some(origin) = press_origin
            && let Some(id) = light_at(lights, origin).filter(|id| !world.is_locked(*id))
            && let Some(pose) = free_pose(world, id)
        {
            // A light's icon is in front of the parts behind it.
            // Grabbed in the plane through the light that faces the ray.
            let ray = screen.ray(origin);
            let normal = ray.direction;
            let grabbed = ray.at((pose.position - ray.origin).dot(normal).max(0.1));
            let others: Vec<(InstanceId, Vec3)> = if selection.contains(&id) {
                parts
                    .iter()
                    .filter(|(other, _)| *other != id)
                    .map(|(other, pose)| (*other, pose.position))
                    .collect()
            } else {
                Vec::new()
            };
            let drag = IconDrag {
                id,
                start: pose.position,
                grab_offset: pose.position - grabbed,
                normal,
                report: None,
            };
            self.drag = Some(Dragging::Icon(drag, others));
            input.grabbed = Some(id);
        } else if body_drag
            && let Some(origin) = press_origin
            && let Some(hit) = pick_unlocked(world, screen.ray(origin))
            // Terrain is ground, not something to carry around.
            && world.get(hit.part).map(|i| i.class()) != Some(axcenity_core::ClassName::Terrain)
            && let Some(mut place) =
                PlaceDrag::start(world, hit.part, screen.ray(origin).at(hit.distance))
        {
            // Grabbing a selected part drags the whole selection with it.
            let others: Vec<(InstanceId, Vec3)> = if selection.contains(&hit.part) {
                parts
                    .iter()
                    .filter(|(id, _)| *id != hit.part)
                    .map(|(id, pose)| (*id, pose.position))
                    .collect()
            } else {
                Vec::new()
            };
            place.skip = others.iter().map(|(id, _)| *id).collect();
            let mut moving = place.skip.clone();
            moving.push(hit.part);
            place.obstacles = snapping::obstacles(world, &moving);
            place.sites = Box::new(snapping::points::sites(world, &moving));
            place.reach =
                screen.studs_per_point(screen.ray(origin).at(hit.distance)) * snapping::SNAP_POINTS;
            self.drag = Some(Dragging::Body(place, others));
            input.grabbed = Some(hit.part);
        }

        input.highlighted = match &self.drag {
            Some(Dragging::Handle(drag)) => Some(drag.handle),
            _ => hovered,
        };
        input.ui_highlighted = match &self.drag {
            Some(Dragging::Ui(drag)) => Some(drag.grip),
            _ => ui_handle,
        };
        input.in_use = self.drag.is_some();
        match (&self.drag, ui_scene.handles) {
            (Some(Dragging::Ui(drag)), Some(handles)) => {
                ui.ctx().set_cursor_icon(handles.cursor(drag.grip));
            }
            (Some(_), _) => ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing),
            (None, Some(handles)) if ui_hovered.is_some() => {
                ui.ctx()
                    .set_cursor_icon(handles.cursor(ui_hovered.expect("checked")));
            }
            (None, _) if hovered.is_some() => ui.ctx().set_cursor_icon(egui::CursorIcon::Grab),
            (None, _) => {}
        }
        if let Some(dragging) = &mut self.drag {
            if !down {
                self.drag = None;
                input.released = true;
            } else if let Some(pointer) = pointer {
                let ray = screen.ray(pointer);
                input.edit = match dragging {
                    Dragging::Handle(drag) => {
                        // Shift turns a ring finely.
                        let at = gizmo::Pointer::on(screen, pointer, shift);
                        drag.update_all(at, &snap).map(|mut parts| {
                            // An Attachment is placed by its offset on its part.
                            crate::constraints::localize(world, &mut parts);
                            drop_free_sizes(world, &mut parts);
                            DragEdit {
                                kind: DragKind::of(drag.handle),
                                parts,
                            }
                        })
                    }
                    Dragging::Body(place, others) => {
                        place.update(world, ray, &snap).map(|position| {
                            let start = place.start;
                            let moved = position
                                .as_vector3()
                                .map_or(Vec3::ZERO, |p| Vec3::new(p.x, p.y, p.z) - start);
                            let mut parts: PartValues = others
                                .iter()
                                .map(|&(id, at)| {
                                    let at = at + moved;
                                    let value = axcenity_core::Vector3::new(at.x, at.y, at.z);
                                    (id, vec![("Position", Value::Vector3(value))])
                                })
                                .collect();
                            parts.push((place.part, vec![("Position", position)]));
                            crate::constraints::localize(world, &mut parts);
                            DragEdit {
                                kind: DragKind::Move,
                                parts,
                            }
                        })
                    }
                    Dragging::Icon(drag, others) => drag.update(ray, &snap).map(|at| {
                        let moved = at - drag.start;
                        drag.report = Some(Box::new(snapping::points::DragReport::new(
                            drag.start,
                            moved,
                            None,
                            Vec::new(),
                            None,
                        )));
                        let value = |at: Vec3| {
                            Value::Vector3(axcenity_core::Vector3::new(at.x, at.y, at.z))
                        };
                        let mut parts: PartValues = others
                            .iter()
                            .map(|&(id, from)| (id, vec![("Position", value(from + moved))]))
                            .collect();
                        parts.push((drag.id, vec![("Position", value(at))]));
                        crate::constraints::localize(world, &mut parts);
                        DragEdit {
                            kind: DragKind::Move,
                            parts,
                        }
                    }),
                    Dragging::Ui(drag) => {
                        input.ui_edit = Some(drag.update(
                            pointer,
                            ui_scene.pixels_per_point,
                            world,
                            &ui_scene.layout.elements,
                            ui_scene.viewport,
                        ));
                        None
                    }
                };
            }
        }
        input.shapes = shapes;
        input
    }

    /// If this frame has a plain left click (not the end of a drag, and not
    /// Alt for orbiting), the UI element or part under the mouse.
    pub(super) fn clicked_part(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        screen: &Screen,
        world: &World,
        ui_scene: &UiScene,
        lights: &[(LightMarker, egui::Pos2)],
    ) -> Option<Clicked> {
        if !response.clicked() || ui.input(|input| input.modifiers.alt) {
            return None;
        }
        let pointer = response.interact_pointer_pos()?;
        Some(Clicked {
            part: ui_scene
                .hit(pointer)
                .or_else(|| light_at(lights, pointer).filter(|id| !world.is_locked(*id)))
                .or_else(|| pick_unlocked(world, screen.ray(pointer)).map(|hit| hit.part))
                // A road's piece or a scattered tree selects what made it.
                .map(|id| axcenity_core::spline::generator_of(world, id).unwrap_or(id)),
            modifiers: ui.input(|input| input.modifiers),
            keep_group: false,
        })
    }

    pub(super) fn handle_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        view_height: f32,
        character: bool,
        wheel_taken: bool,
    ) {
        let ctx = ui.ctx();
        // Read this *before* `ui.input`: egui locks its context while the
        // `ui.input` closure runs, and `drag_delta` takes the same lock, so
        // calling it inside the closure waits for itself forever (a deadlock).
        let drag = response.drag_delta();
        let (motion, mut scroll, mut wheel, alt, shift, seconds, focused, time) =
            ui.input(|input| {
                // Key presses and releases, by key position rather than by
                // letter, so WASD sits in the same place on AZERTY keyboards.
                for event in &input.events {
                    if let egui::Event::Key {
                        key,
                        physical_key,
                        pressed,
                        ..
                    } = event
                    {
                        set_move_key(&mut self.keys, physical_key.unwrap_or(*key), *pressed);
                    }
                }
                (
                    // Raw mouse movement keeps coming while the cursor is held
                    // still; it's missing on some platforms, so fall back to the drag.
                    input.pointer.motion().unwrap_or(drag),
                    input.smooth_scroll_delta.y,
                    // The wheel without egui's smoothing, so each notch is
                    // exactly one speed step. egui turns Shift+wheel into
                    // sideways scrolling (and macOS already does), so add both.
                    {
                        let raw = crate::wheel::raw(input);
                        raw.x + raw.y
                    },
                    input.modifiers.alt,
                    input.modifiers.shift,
                    input.stable_dt.min(MAX_FRAME_SECONDS),
                    input.focused,
                    input.time,
                )
            });
        // The wheel scrolled a ScrollingFrame instead.
        if wheel_taken {
            scroll = 0.0;
            wheel = 0.0;
        }
        // Key releases aren't reported while another window has focus, so
        // let go of everything to avoid the camera drifting on its own.
        if !focused {
            self.keys = MoveKeys::default();
        }
        self.keys.slow = shift;

        let looking = response.dragged_by(egui::PointerButton::Secondary);
        if looking != self.looking {
            self.looking = looking;
            capture_mouse(ctx, looking);
        }
        // A character controller owns the view. Look and the wheel's zoom
        // are applied on the next character step; the fly camera stays
        // where Stop will find it.
        if character {
            if looking {
                self.character_look.0 += motion.x;
                self.character_look.1 += motion.y;
            }
            if response.hovered() || looking {
                self.character_zoom += wheel / POINTS_PER_NOTCH;
            }
            return;
        }
        if looking {
            self.camera.look(motion.x, motion.y);
        } else if alt
            && self.spline_drag.is_none()
            && response.dragged_by(egui::PointerButton::Primary)
        {
            self.camera.orbit(motion.x, motion.y);
        } else if response.dragged_by(egui::PointerButton::Middle) {
            self.camera.pan(drag.x, drag.y, view_height);
        }
        // While flying (right button held) or holding Shift, the wheel sets
        // the flying speed, like Unity and Godot. Otherwise it zooms.
        let over_viewport = response.contains_pointer() || looking;
        if over_viewport && (looking || shift) {
            if wheel != 0.0 {
                self.camera.change_speed(wheel / POINTS_PER_NOTCH);
                self.speed_changed_at = Some(time);
            }
        } else if response.hovered() && scroll != 0.0 {
            self.camera.zoom(scroll * ZOOM_PER_POINT);
        }

        // Keys fly the camera while the mouse is over the viewport, unless
        // some text box is taking the typing.
        let flying = (response.contains_pointer() || looking) && !ctx.egui_wants_keyboard_input();
        let keys = &self.keys;
        let moving = keys.forward || keys.back || keys.left || keys.right || keys.up || keys.down;
        if flying && moving {
            self.camera.update(keys, seconds);
            // egui only redraws when something happens, and holding a key
            // down isn't an event, so ask for the next frame.
            ctx.request_repaint();
        }
    }
}
