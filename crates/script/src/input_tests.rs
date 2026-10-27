//! Tests for `UserInputService`'s mouse, touch and controller input, the
//! input map's actions and rebinds, and `ContextActionService`.

use axcenity_core::input::{Input, InputEvent, InputKind};
use axcenity_core::{ClassName, KeyCode, MouseBehavior, UserInputState, UserInputType, World};

use super::*;

fn local_script(runtime: &mut ScriptRuntime, world: &mut World, source: &str) {
    let script = world.create(ClassName::LocalScript).unwrap();
    world
        .set_parent(script, Some(world.starter_player_scripts()))
        .unwrap();
    runtime.start_script(world, script, "input.client.lua", source);
}

fn lines(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Error => format!("error: {}", line.text),
            _ => line.text,
        })
        .collect()
}

fn key(key: KeyCode, pressed: bool) -> InputEvent {
    InputEvent::new(InputKind::Key { key, pressed })
}

fn click(position: [f32; 2], pressed: bool) -> InputKind {
    InputKind::MouseButton {
        button: UserInputType::MouseButton1,
        pressed,
        position,
    }
}

#[test]
fn a_local_script_gets_mouse_button_1_with_the_clicks_position_and_whether_the_ui_took_it() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    local_script(
        &mut runtime,
        &mut world,
        r#"
        local uis = game:GetService("UserInputService")
        uis.InputBegan:Connect(function(input, processed)
            print(input.UserInputType.Name, input.UserInputState.Name, input.Position.X, input.Position.Y, processed, input.KeyCode.Name)
            print(uis:IsMouseButtonPressed(Enum.UserInputType.MouseButton1), uis:GetMouseLocation())
        end)
        uis.InputEnded:Connect(function(input, processed)
            print("up", input.UserInputType.Name, processed)
        end)
        "#,
    );
    runtime.input_events(&mut world, &[InputEvent::new(click([320.0, 200.0], true))]);
    runtime.input_events(&mut world, &[InputEvent::new(click([320.0, 200.0], false))]);
    // A click the UI took (on a TextButton): scripts still hear it.
    runtime.input_events(
        &mut world,
        &[InputEvent::processed(click([40.0, 30.0], true))],
    );
    assert_eq!(
        lines(&mut runtime),
        [
            "MouseButton1\tBegin\t320\t200\tfalse\tUnknown",
            "true\t320, 200",
            "up\tMouseButton1\tfalse",
            "MouseButton1\tBegin\t40\t30\ttrue\tUnknown",
            "true\t40, 30",
        ]
    );
}

#[test]
fn mouse_movement_wheel_touch_and_controllers_fire_their_events() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    local_script(
        &mut runtime,
        &mut world,
        r#"
        local uis = game:GetService("UserInputService")
        uis.InputChanged:Connect(function(input)
            print("changed", input.UserInputType.Name, input.KeyCode.Name, input.Position, input.Delta)
        end)
        uis.InputBegan:Connect(function(input)
            print("began", input.UserInputType.Name, input.KeyCode.Name)
        end)
        for _, name in {"TouchStarted", "TouchMoved", "TouchEnded"} do
            uis[name]:Connect(function(input) print(name, input.Position.X) end)
        end
        uis.GamepadConnected:Connect(function(pad) print("connected", pad) end)
        "#,
    );
    let pad = 0;
    runtime.input_events(
        &mut world,
        &[
            InputKind::MouseMove {
                position: [10.0, 20.0],
                delta: [3.0, -2.0],
            }
            .into(),
            InputKind::Wheel {
                position: [10.0, 20.0],
                notches: 1.0,
            }
            .into(),
            InputKind::Touch {
                id: 7,
                state: UserInputState::Begin,
                position: [50.0, 60.0],
            }
            .into(),
            InputKind::Touch {
                id: 7,
                state: UserInputState::Change,
                position: [55.0, 60.0],
            }
            .into(),
            InputKind::Touch {
                id: 7,
                state: UserInputState::End,
                position: [55.0, 60.0],
            }
            .into(),
            InputKind::PadConnected(pad).into(),
            InputKind::PadButton {
                pad,
                key: KeyCode::ButtonA,
                pressed: true,
            }
            .into(),
            InputKind::PadStick {
                pad,
                stick: KeyCode::Thumbstick1,
                position: [0.0, 1.0],
            }
            .into(),
        ],
    );
    local_script(
        &mut runtime,
        &mut world,
        r#"
        local uis = game:GetService("UserInputService")
        print(uis:IsGamepadButtonDown(Enum.UserInputType.Gamepad1, Enum.KeyCode.ButtonA), #uis:GetConnectedGamepads())
        for _, input in uis:GetGamepadState(Enum.UserInputType.Gamepad1) do
            if input.UserInputState == Enum.UserInputState.Begin then
                print("held", input.KeyCode.Name, input.Position)
            end
        end
        "#,
    );
    assert_eq!(
        lines(&mut runtime),
        [
            "changed\tMouseMovement\tUnknown\t10, 20, 0\t3, -2, 0",
            "changed\tMouseWheel\tUnknown\t10, 20, 1\t0, 0, 0",
            "TouchStarted\t50",
            "began\tTouch\tUnknown",
            "TouchMoved\t55",
            "changed\tTouch\tUnknown\t55, 60, 0\t0, 0, 0",
            "TouchEnded\t55",
            "connected\tEnum.UserInputType.Gamepad1",
            "began\tGamepad1\tButtonA",
            "changed\tGamepad1\tThumbstick1\t0, 1, 0\t0, 1, 0",
            "true\t1",
            "held\tButtonA\t0, 0, 0",
            "held\tThumbstick1\t0, 1, 0",
        ]
    );
}

#[test]
fn with_the_mouse_locked_in_the_centre_get_mouse_delta_turns_the_camera() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    local_script(
        &mut runtime,
        &mut world,
        r#"
        local uis = game:GetService("UserInputService")
        uis.MouseBehavior = Enum.MouseBehavior.LockCenter
        uis.MouseIconEnabled = false
        local camera = workspace.CurrentCamera
        game:GetService("RunService").Heartbeat:Connect(function()
            local delta = uis:GetMouseDelta()
            camera.Orientation = camera.Orientation - Vector3.new(delta.Y, delta.X, 0) * 0.1
        end)
        "#,
    );
    assert_eq!(runtime.mouse(), (MouseBehavior::LockCenter, false));
    // The host holds the cursor in the middle, and reports how far it moved.
    let centre = [640.0, 360.0];
    let frame = |runtime: &mut ScriptRuntime, world: &mut World, delta: [f32; 2]| {
        runtime.set_camera(CameraState::default());
        let moved = InputKind::MouseMove {
            position: centre,
            delta,
        };
        runtime.input_events(world, &[moved.into()]);
        runtime.step(world, 1.0 / 60.0);
        runtime.heartbeat(world, 1.0 / 60.0);
        runtime.take_camera_move()
    };
    let turned = frame(&mut runtime, &mut world, [100.0, -50.0]).unwrap();
    assert_eq!(turned.orientation, Vector3::new(5.0, -10.0, 0.0));
    // A frame without movement has no delta, so the camera stays.
    runtime.set_camera(CameraState::default());
    runtime.input_events(&mut world, &[]);
    runtime.heartbeat(&mut world, 1.0 / 60.0);
    assert_eq!(
        runtime.take_camera_move().unwrap().orientation,
        Vector3::new(0.0, 0.0, 0.0)
    );
    assert!(lines(&mut runtime).is_empty());
}

#[test]
fn actions_read_the_input_map_and_a_rebind_survives_saving_and_loading_a_slot() {
    let folder = std::env::temp_dir().join(format!("axcenity-input-rebind-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_save_folder(Some(folder.clone()));
    local_script(
        &mut runtime,
        &mut world,
        r#"
        local uis = game:GetService("UserInputService")
        local function check()
            print(uis:IsActionPressed("Jump"), uis:GetActionAxis("Move"), table.concat(uis:GetActionInputs("Jump"), " "))
        end
        -- Checked again once W is down.
        uis.InputBegan:Connect(function(input)
            if input.KeyCode == Enum.KeyCode.W then check() end
        end)
        check()
        uis:RebindAction("Jump", {Enum.KeyCode.E, "ButtonY"})
        check()
        print(pcall(uis.RebindAction, uis, "Fly", {}))
        print(pcall(uis.RebindAction, uis, "Jump", {"Spacebar"}))
        print(pcall(uis.IsActionPressed, uis, "Fly"))
        "#,
    );
    runtime.input_events(&mut world, &[key(KeyCode::E, true), key(KeyCode::W, true)]);
    let output = lines(&mut runtime);
    assert_eq!(output[0], "false\t0, 0\tSpace ButtonA");
    assert_eq!(output[1], "false\t0, 0\tE ButtonY");
    assert!(
        output[2].contains("no action named \"Fly\""),
        "{}",
        output[2]
    );
    assert!(
        output[3].contains("\"Spacebar\" isn't one"),
        "{}",
        output[3]
    );
    assert!(
        output[4].contains("IsActionPressed: there's no action named \"Fly\""),
        "{}",
        output[4]
    );
    assert_eq!(output[5], "true\t0, 1\tE ButtonY");
    assert_eq!(
        runtime.input_map().action("Jump").unwrap().inputs,
        [Input::Key(KeyCode::E), Input::Key(KeyCode::ButtonY)]
    );

    // Saved, rebound back, then loaded: E again.
    local_script(
        &mut runtime,
        &mut world,
        r#"
        local uis = game:GetService("UserInputService")
        local saves = game:GetService("SaveService")
        saves:SaveSlot(1)
        uis:RebindAction("Jump", {Enum.KeyCode.Space})
        print(table.concat(uis:GetActionInputs("Jump"), " "))
        saves:LoadSlot(1)
        print(table.concat(uis:GetActionInputs("Jump"), " "))
        "#,
    );
    assert_eq!(lines(&mut runtime), ["Space", "E ButtonY"]);

    // A new game run that loads the slot gets the rebind too.
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_save_folder(Some(folder.clone()));
    assert_eq!(
        runtime.input_map().action("Jump").unwrap().inputs,
        [Input::Key(KeyCode::Space), Input::Key(KeyCode::ButtonA)]
    );
    local_script(
        &mut runtime,
        &mut world,
        "game:GetService('SaveService'):LoadSlot(1)",
    );
    assert!(lines(&mut runtime).is_empty());
    assert_eq!(
        runtime.input_map().action("Jump").unwrap().inputs,
        [Input::Key(KeyCode::E), Input::Key(KeyCode::ButtonY)]
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_higher_priority_action_hides_the_lower_one_until_it_is_unbound() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    local_script(
        &mut runtime,
        &mut world,
        r#"
        local cas = game:GetService("ContextActionService")
        local function report(name, state, input)
            print(name, state.Name, input.KeyCode.Name)
        end
        cas:BindActionAtPriority("Menu", report, false, 3000, Enum.KeyCode.E)
        cas:BindAction("Use", report, true, Enum.KeyCode.E, Enum.UserInputType.MouseButton1)
        local info = cas:GetBoundActionInfo("Use")
        print(info.priorityLevel, info.createTouchButton, #info.inputTypes, next(cas:GetBoundActionInfo("Nothing")))
        game:GetService("UserInputService").InputBegan:Connect(function(input)
            if input.KeyCode == Enum.KeyCode.U then cas:UnbindAction("Menu") end
        end)
        "#,
    );
    let tap = |runtime: &mut ScriptRuntime, world: &mut World| {
        runtime.input_events(world, &[key(KeyCode::E, true)]);
        runtime.input_events(world, &[key(KeyCode::E, false)]);
    };
    tap(&mut runtime, &mut world);
    // Typed into a TextBox (the UI took it): no action.
    runtime.input_events(
        &mut world,
        &[InputEvent::processed(InputKind::Key {
            key: KeyCode::E,
            pressed: true,
        })],
    );
    runtime.input_events(&mut world, &[key(KeyCode::E, false)]);
    runtime.input_events(&mut world, &[key(KeyCode::U, true)]);
    tap(&mut runtime, &mut world);
    runtime.input_events(&mut world, &[InputEvent::new(click([1.0, 1.0], true))]);
    assert_eq!(
        lines(&mut runtime),
        [
            "2000\ttrue\t2\tnil",
            "Menu\tBegin\tE",
            "Menu\tEnd\tE",
            "Use\tBegin\tE",
            "Use\tEnd\tE",
            "Use\tBegin\tUnknown",
        ]
    );
}

#[test]
fn context_action_service_only_works_in_a_local_script() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(
        &mut world,
        script,
        "actions.server.lua",
        "local cas = game:GetService('ContextActionService')",
    );
    let output = lines(&mut runtime);
    assert!(
        output[0].contains("ContextActionService only works in a LocalScript"),
        "{output:?}"
    );
}
