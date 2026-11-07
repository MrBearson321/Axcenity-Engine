# Splines

Phase 73, like Unreal's Spline component. A **Spline** is a curve through its **SplinePoint** children; a **SplineFollower** carries a part along one, a NavigationAgent patrols one, and a Sequence's camera rides one. Example: `examples/hello-splines`.

## The curve

`core::spline` (`crates/core/src/spline.rs`), shared by `script`, `nav`, `anim` and `net`.

- A Spline has `Closed` (off). It has no position: each SplinePoint's `Position` is in the world, and its place among the Spline's children is its place along the curve. A closed curve goes on from the last point back to the first.
- Each stretch between two points is a cubic Bézier. A point's `TangentMode` (`Enum.SplineTangentMode`) makes its handles: **Auto** passes smoothly through it (Catmull-Rom, from its neighbours; an open end points at its one neighbour), **Linear** points them at its neighbours, so two Linear points are joined by a straight line, and **Free** uses `InTangent` and `OutTangent`: offsets in studs from the point to each handle ((-2, 0, 0) and (2, 0, 0) to start with).
- `SplineCurve::of(world, spline)` measures it: each stretch sampled 64 times (`SAMPLES`) into a table of lengths, so a distance along the curve is an even speed however the points are spaced. A distance past an end wraps round a closed curve and is held at the end of an open one (`wrap`). One point is a curve of no length at that point; no points is no curve.
- Facing along it (`orientation_at_distance`, `facing`) is upright: pitched and turned to the curve's direction, never rolled.

## Lua and blocks

`script/src/spline.rs`. A Spline's `Length` is worked out from its points (read-only). `GetPositionAt(t)` is the point `t` (0 to 1) of the way along by distance; `GetCFrameAt(distance)` is where the curve is `distance` studs along, facing along it; `GetClosestDistance(position)` is how far along its point nearest `position` is. Each errors on a Spline with no SplinePoint. `agent:Patrol(spline)` sets a NavigationAgent's `PatrolSpline` (nil stops). Test: `get_cframe_at_half_the_length_lands_on_a_known_curves_midpoint` (`script/src/spline_tests.rs`); the curve's own tests are in `core/src/spline.rs`.

Blocks ([../scripting/blocks.md](../scripting/blocks.md)): **put *object* on *spline* at *n* studs** (`object:PivotTo(spline:GetCFrameAt(n))`), **patrol *agent* along *spline*** (`agent:Patrol(spline)`), **when *follower* reaches the *End/Start*** (`follower.Reached:Connect(function(which) if which == "End" then … end end)`), and the reporters **point *t* of the way along *spline*** and **distance along *spline* nearest *position***. `Length` is the *property* of *object* reporter.

## SplineFollower

A mover parented to a Part or Model: `Spline`, `Speed` (8 studs a second), `Mode` (`Enum.SplineFollowMode`: **Loop** goes on from the start, **PingPong**, the default, turns back at each end, **Once** stops at the end), `FaceAlong` (off) and `Enabled` (on). Its `Reached` event fires with `"End"` or `"Start"` as it gets to an end (a Loop round a Closed spline as it comes back to the start).

- `script::spline::Followers`, owned by the `ScriptRuntime`, steps them in frame step 2, after both sides' scripts and before Sequences (`ScriptRuntime::step`), and only on the server: a device following one sees the server's motion. Runs aren't saved, so Play starts each where its parent stands: at the curve's point nearest the parent's pivot (a part's CFrame; a Model's PrimaryPart, else the middle of its parts' box).
- It moves the part and the parts welded to it (`physics::assembly::connected_parts`), or every part in the Model, keeping their places relative to each other. Without `FaceAlong` they keep the turn they had; with it they face the way they go (back along the curve on a PingPong's way back). Move anchored parts: physics doesn't push what stands on a moved anchored part, and a character only rides anchored parts (below).
- Tests: `a_ping_pong_follower_goes_to_the_end_and_back_firing_reached_at_each`, `loop_starts_again_and_once_stops_at_the_end`, `a_follower_moves_a_models_parts_together_and_face_along_turns_them`.

## Riding

A character controller rides the anchored part under the middle of its feet (`character`: `Footing`, `carry`): each step, before walking, a grounded character moves as far as that part moved since its last step and round it as far as it turned about Y, then walks from there. So it trails the part by one frame, smoothly. A part that jumps more than 20 studs (`MAX_CARRY`) was put somewhere, and isn't followed. Unanchored parts aren't ridden: the character is held still for physics, so following one down would press into it. Tests: `a_standing_character_rides_a_platform_that_moves_and_rises_under_it`, `a_platform_that_turns_carries_the_character_round_with_it` (`character/src/tests.rs`), and the player's `the_lift_ping_pongs_along_its_spline_carrying_the_character_smoothly` (`player/src/spline_tests.rs`, hello-splines in the player's frame order).

In multiplayer the character rides the lift as its device shows it, and the server's `CharacterCheck` takes off how far the SplineFollower-moved part under the character went since the last update before checking its own movement against `WalkSpeed` ([networking.md](networking.md)). Test: `a_ping_pong_lift_carries_a_joined_players_character_smoothly_without_corrections` (`net/src/character_tests.rs`: a lift at 30 studs a second, nearly twice WalkSpeed).

## Patrols and cameras

- A NavigationAgent with a `PatrolSpline` walks to its points in order with no script, along the navigation mesh: round again on a Closed spline, back and forth on an open one, starting at the point nearest it. It goes on once it reached a point (or can't reach it). See [navigation.md](navigation.md). Tests: `an_agent_patrols_a_closed_spline_round_and_round_with_no_script` (`nav/src/agent_tests.rs`) and the player's `the_guard_patrols_its_closed_route_and_the_flyby_camera_rides_its_rail`.
- A Sequence's camera cut bound to a Spline rides it from its start at the cut to its end when the next cut (or the end) takes over, facing along it, at the default field of view ([../project/sequences.md](../project/sequences.md)). Test: `a_camera_cut_bound_to_a_spline_rides_it_from_the_cut_to_the_next` (`anim/src/sequence/tests.rs`).

## In the editor

`editor/src/splines.rs` (drawing and hit tests), `app/splines.rs` (the edits), `viewport/input.rs` (`spline_input`).

- While editing, every Spline in the Workspace is drawn as its curve with a dot at each point; in Play and in the game it isn't drawn.
- **Point editing**: clicking a curve selects its Spline, and while a Spline or one of its points is selected its points get handles (`EditorApp::spline_editing`). Dragging a point puts it on the surface under the mouse (`scene::pick_hit`), one "Move" undo step per drag. **Alt**-drag on a point adds a point after it where it's let go ("Add Spline Point", one step); Alt+left-drag elsewhere still orbits the camera. **Delete** removes the selected point, as for anything selected. A Free point's handles are drawn as lines; set them in Properties.
- **Insert ▸ Vehicle** (and the Explorer's and Model tab's menus) has **Spline** (two points either side of the camera's focus, one undo step, selected for editing) and **Spline Follower** (into the selected part or Model, following the first Spline).
- Tests: `a_spline_point_drags_onto_the_surface_under_the_mouse_and_alt_drag_adds_one` (`viewport/tests.rs`), `dragging_a_point_adding_one_and_deleting_one_are_each_one_undo_step` and `insert_spline_makes_two_points_in_one_step_and_selects_it_for_editing` (`app/spline_tests.rs`).

## Not yet

Handles of Free points aren't dragged in the viewport. A Spline doesn't move as a whole (its points do). Unanchored parts on a SplineFollower are moved without telling physics their speed, so loose crates on a moving platform aren't carried.
