//! Tests for attributes, tags and Value objects (Phase 29).

use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};

use super::*;

fn start(runtime: &mut ScriptRuntime, world: &mut World, name: &str, source: &str) -> InstanceId {
    let script = world.create(ClassName::Script).unwrap();
    world.set_name(script, name).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(world, script, &format!("scripts/{name}.server.lua"), source);
    script
}

fn lines(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        // `print` puts tabs between its values.
        .map(|line| match line.kind {
            OutputKind::Print => line.text.replace('\t', " "),
            OutputKind::Error => format!("error: {}", line.text),
            OutputKind::Warning => format!("warning: {}", line.text),
        })
        .collect()
}

fn part(world: &mut World, name: &str) -> InstanceId {
    let part = world.create(ClassName::Part).unwrap();
    world.set_name(part, name).unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    part
}

#[test]
fn an_attribute_set_in_the_editor_is_read_in_play_and_its_signal_fires_when_another_script_changes_it()
 {
    let mut world = World::new();
    let door = part(&mut world, "Door");
    // As the Properties panel's Attributes section sets it.
    world
        .set_attribute(door, "Speed", Some(Value::Number(12.0)))
        .unwrap();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "Reader",
        r#"
        local door = workspace.Door
        print("speed", door:GetAttribute("Speed"))
        door:GetAttributeChangedSignal("Speed"):Connect(function()
            print("speed now", door:GetAttribute("Speed"))
        end)
        door.AttributeChanged:Connect(function(name)
            print("changed", name)
        end)
        "#,
    );
    start(
        &mut runtime,
        &mut world,
        "Writer",
        r#"
        local door = workspace.Door
        door:SetAttribute("Speed", 20)
        door:SetAttribute("Open", true)
        door:SetAttribute("Open", true) -- the same value: no event
        door:SetAttribute("Open", nil)
        "#,
    );
    assert_eq!(
        lines(&mut runtime),
        [
            "speed 12",
            "changed Speed",
            "speed now 20",
            "changed Open",
            "changed Open",
        ]
    );
    assert_eq!(
        world.get(door).unwrap().attribute("Speed"),
        Some(&Value::Number(20.0))
    );
    assert_eq!(world.get(door).unwrap().attribute("Open"), None);
}

#[test]
fn every_kind_of_attribute_value_round_trips_and_others_are_refused() {
    let mut world = World::new();
    part(&mut world, "P");
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "Kinds",
        r#"
        local p = workspace.P
        local values = {
            On = true, Count = 3, Label = "hi", Spawn = Vector3.new(1, 2, 3),
            Size2 = Vector2.new(4, 5), Tint = Color3.new(1, 0, 0),
            Box = UDim2.new(0.5, 1, 0, 2), Pivot = CFrame.new(1, 2, 3) * CFrame.Angles(0, math.rad(90), 0),
            Mat = Enum.Material.Neon, Key = Enum.KeyCode.E,
        }
        for name, value in values do
            p:SetAttribute(name, value)
        end
        local same = 0
        for name, value in p:GetAttributes() do
            if typeof(value) == "CFrame" then
                if (value.Position - values[name].Position).Magnitude < 1e-5 then same += 1 end
            elseif value == values[name] then
                same += 1
            else
                print("differs", name, value)
            end
        end
        print("same", same)
        print(p:GetAttribute("Missing"))
        print(pcall(p.SetAttribute, p, "Self", p))
        print(pcall(p.SetAttribute, p, "bad name", 1))
        "#,
    );
    let out = lines(&mut runtime);
    assert_eq!(out[0], "same 10", "{out:?}");
    assert_eq!(out[1], "nil");
    assert!(
        out[2].starts_with("false") && out[2].contains("not a Part"),
        "{out:?}"
    );
    assert!(
        out[3].starts_with("false") && out[3].contains("bad name"),
        "{out:?}"
    );
}

#[test]
fn get_tagged_and_the_added_signal_handle_existing_and_newly_cloned_lava() {
    let mut world = World::new();
    let lava = part(&mut world, "Lava");
    world.add_tag(lava, "Lava").unwrap();
    let other = part(&mut world, "Floor");
    world.add_tag(other, "Floor").unwrap();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "LavaHandler",
        r#"
        local CollectionService = game:GetService("CollectionService")
        local count = 0
        local function handle(part)
            count += 1
            part:SetAttribute("Handled", count)
            print("lava", part.Name, count)
        end
        for _, part in CollectionService:GetTagged("Lava") do
            handle(part)
        end
        CollectionService:GetInstanceAddedSignal("Lava"):Connect(handle)
        CollectionService:GetInstanceRemovedSignal("Lava"):Connect(function(part)
            print("gone", part.Name)
        end)
        "#,
    );
    start(
        &mut runtime,
        &mut world,
        "Spawner",
        r#"
        local copy = workspace.Lava:Clone()
        copy.Name = "Lava2"
        print("copy tags", table.concat(copy:GetTags(), ","))
        copy.Parent = workspace
        -- A tag added in the game counts too, and the shortcut works.
        workspace.Floor:AddTag("Lava")
        "#,
    );
    runtime.step(&mut world, 0.1);
    // Instances that joined in the same round come in tree order.
    assert_eq!(
        lines(&mut runtime),
        [
            "lava Lava 1",
            "copy tags Lava",
            "lava Floor 2",
            "lava Lava2 3"
        ]
    );
    let copy = world.find_first_child(world.workspace(), "Lava2").unwrap();
    assert_eq!(
        world.get(copy).unwrap().attribute("Handled"),
        Some(&Value::Number(3.0))
    );

    start(
        &mut runtime,
        &mut world,
        "Remover",
        r#"
        workspace.Lava2:Destroy()
        game:GetService("CollectionService"):RemoveTag(workspace.Floor, "Lava")
        print(workspace.Floor:HasTag("Lava"), #game:GetService("CollectionService"):GetTagged("Lava"))
        "#,
    );
    runtime.step(&mut world, 0.1);
    assert_eq!(lines(&mut runtime), ["gone Lava2", "false 1", "gone Floor"]);
}

#[test]
fn value_objects_hold_their_value_and_changed_fires_with_the_new_value() {
    let mut world = World::new();
    let score = world.create(ClassName::IntValue).unwrap();
    world.set_name(score, "Score").unwrap();
    world.set_parent(score, Some(world.workspace())).unwrap();
    let target = world.create(ClassName::ObjectValue).unwrap();
    world.set_name(target, "Target").unwrap();
    world.set_parent(target, Some(world.workspace())).unwrap();
    let door = part(&mut world, "Door");
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "Values",
        r#"
        local score = workspace.Score
        score.Changed:Connect(function(value) print("score", value) end)
        workspace.Target.Changed:Connect(function(value) print("target", value) end)
        score.Value = 2.6
        score.Name = "Points" -- not the Value: Changed stays quiet
        workspace.Target.Value = workspace.Door
        print(workspace.Target.Value == workspace.Door)

        local kinds = {
            NumberValue = 1.5, StringValue = "s", BoolValue = true,
            Vector3Value = Vector3.new(1, 2, 3), Color3Value = Color3.new(0, 1, 0),
        }
        for class, value in kinds do
            local object = Instance.new(class)
            object.Value = value
            assert(object.Value == value, class)
        end
        local cf = Instance.new("CFrameValue")
        cf.Value = CFrame.new(1, 2, 3)
        print(cf.Value.Position)
        print(pcall(function() score.Value = "x" end))
        "#,
    );
    let out = lines(&mut runtime);
    assert_eq!(out[..2], ["true", "1, 2, 3"], "{out:?}");
    assert!(out[2].starts_with("false"), "{out:?}");
    assert_eq!(out[3..], ["score 3", "target Door"], "{out:?}");
    assert_eq!(
        world.get(target).unwrap().property("Value"),
        Some(&Value::Instance(Some(door)))
    );

    // Pointing at a destroyed Instance reads as nil.
    start(
        &mut runtime,
        &mut world,
        "Destroyer",
        "workspace.Door:Destroy() print(workspace.Target.Value)",
    );
    assert_eq!(lines(&mut runtime), ["nil"]);
    let _ = Vector3::default();
}
