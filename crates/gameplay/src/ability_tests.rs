//! Tests for Phase 76: Abilities and Effects, stepped as a host steps them.

use axcenity_assets::{Assets, Cell, ColumnType, DataTable};
use axcenity_core::input::{Action, InputEvent, InputKind, parse_binding};
use axcenity_core::{ClassName, InstanceId, KeyCode, Value, World};

use axcenity_script::{OutputKind, ScriptRuntime};

use crate::{ability_bar, script_runtime, step_abilities};

const FRAME: f64 = 1.0 / 60.0;

fn start(world: &mut World, runtime: &mut ScriptRuntime, source: &str) {
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(world, script, "scripts/test.server.lua", source);
}

fn start_local(world: &mut World, runtime: &mut ScriptRuntime, source: &str) {
    let script = world.create(ClassName::LocalScript).unwrap();
    world
        .set_parent(script, Some(world.starter_player_scripts()))
        .unwrap();
    runtime.start_script(world, script, "scripts/test.client.lua", source);
}

fn output(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Error => format!("error: {}", line.text),
            _ => line.text.replace('\t', " "),
        })
        .collect()
}

fn model(world: &mut World, name: &str) -> (InstanceId, InstanceId) {
    let model = world.create(ClassName::Model).unwrap();
    world.set_name(model, name).unwrap();
    world.set_parent(model, Some(world.workspace())).unwrap();
    let health = world.create(ClassName::Health).unwrap();
    world.set_name(health, "Health").unwrap();
    world
        .set_property(health, "RegenRate", Value::Number(0.0))
        .unwrap();
    world.set_parent(health, Some(model)).unwrap();
    let controller = world.create(ClassName::ThirdPersonController).unwrap();
    world.set_parent(controller, Some(model)).unwrap();
    (model, health)
}

fn number(world: &World, id: InstanceId, name: &str) -> f64 {
    world
        .get(id)
        .unwrap()
        .property(name)
        .and_then(Value::as_number)
        .unwrap()
}

fn attribute(world: &World, id: InstanceId, name: &str) -> f64 {
    world
        .get(id)
        .unwrap()
        .attribute(name)
        .and_then(Value::as_number)
        .unwrap()
}

/// The player's character (a Model with 100 Mana), the local Player whose
/// Character it is, and a Target to hit. The input map has a Fireball
/// action on Q.
struct Game {
    world: World,
    runtime: ScriptRuntime,
    character: InstanceId,
    target_health: InstanceId,
}

impl Game {
    fn new() -> Self {
        let mut world = World::new();
        let (character, _) = model(&mut world, "Character");
        world
            .set_attribute(character, "Mana", Some(Value::Number(100.0)))
            .unwrap();
        let (_, target_health) = model(&mut world, "Target");
        let mut runtime = script_runtime();
        let mut map = axcenity_core::input::InputMap::default();
        map.actions.push(Action {
            name: "Fireball".to_owned(),
            axis: false,
            inputs: parse_binding("Q").unwrap(),
        });
        runtime.set_input_map(map);
        // The first step joins the player; their character is ours.
        runtime.step(&mut world, 0.0);
        let player = runtime.local_player().unwrap();
        world
            .set_property(player, "Character", Value::Instance(Some(character)))
            .unwrap();
        Game {
            world,
            runtime,
            character,
            target_health,
        }
    }

    /// One frame as the player runs one: input, abilities, scripts and
    /// the health step.
    fn frame(&mut self, events: &[InputEvent]) {
        let world = &mut self.world;
        self.runtime.input_events(world, events);
        step_abilities(&mut self.runtime, world, FRAME);
        self.runtime.step(world, FRAME);
        self.runtime.step_health(world, FRAME);
    }

    fn seconds(&mut self, seconds: f64) {
        for _ in 0..(seconds / FRAME).round() as usize {
            self.frame(&[]);
        }
    }

    fn tap(&mut self, key: KeyCode) {
        self.frame(&[InputEvent::new(InputKind::Key { key, pressed: true })]);
        self.frame(&[InputEvent::new(InputKind::Key {
            key,
            pressed: false,
        })]);
    }

    fn mana(&self) -> f64 {
        attribute(&self.world, self.character, "Mana")
    }

    fn target(&self) -> f64 {
        number(&self.world, self.target_health, "Health")
    }
}

/// The Fireball: 20 Mana, 3 seconds of cooldown, Q, blocked by Stunned,
/// and a Script that does only the damage.
fn fireball(game: &mut Game) -> InstanceId {
    let world = &mut game.world;
    let ability = world.create(ClassName::Ability).unwrap();
    world.set_name(ability, "Fireball").unwrap();
    for (name, value) in [
        ("Cooldown", Value::Number(3.0)),
        ("CostAttribute", Value::String("Mana".to_owned())),
        ("CostAmount", Value::Number(20.0)),
        ("ActivationKey", Value::String("Fireball".to_owned())),
        ("BlockedByTags", Value::String("Stunned".to_owned())),
    ] {
        world.set_property(ability, name, value).unwrap();
    }
    world.set_parent(ability, Some(game.character)).unwrap();
    start(
        &mut game.world,
        &mut game.runtime,
        r#"
workspace.Character.Fireball.Activated:Connect(function()
    workspace.Target.Health:TakeDamage(25)
end)
"#,
    );
    ability
}

#[test]
fn a_fireball_costs_20_mana_cools_down_for_3_seconds_and_cant_activate_while_stunned() {
    let mut game = Game::new();
    let ability = fireball(&mut game);

    game.tap(KeyCode::Q);
    assert_eq!(game.mana(), 80.0);
    assert_eq!(game.target(), 75.0, "the script's damage");
    let left = number(&game.world, ability, "CooldownLeft");
    assert!(left > 2.9 && left <= 3.0, "{left}");
    let bar = ability_bar(&game.runtime, &game.world);
    assert_eq!(bar.len(), 1);
    assert_eq!(bar[0].key, "Q");
    assert!(
        bar[0].sweep() > 0.95,
        "the sweep starts full: {}",
        bar[0].sweep()
    );

    // Pressed again a second later: still cooling down.
    game.seconds(1.0);
    game.tap(KeyCode::Q);
    assert_eq!(game.mana(), 80.0);
    assert_eq!(game.target(), 75.0);
    let half = ability_bar(&game.runtime, &game.world)[0].sweep();
    assert!(half > 0.55 && half < 0.7, "{half}");

    // Held down, it activates once per press.
    game.seconds(2.1);
    assert_eq!(number(&game.world, ability, "CooldownLeft"), 0.0);
    game.frame(&[InputEvent::new(InputKind::Key {
        key: KeyCode::Q,
        pressed: true,
    })]);
    game.seconds(3.5);
    assert_eq!(game.mana(), 60.0, "once for the one press");
    game.frame(&[InputEvent::new(InputKind::Key {
        key: KeyCode::Q,
        pressed: false,
    })]);

    // Stunned, it can't.
    game.world.add_tag(game.character, "Stunned").unwrap();
    game.tap(KeyCode::Q);
    assert_eq!(game.mana(), 60.0);
    assert_eq!(game.target(), 50.0);
    game.world.remove_tag(game.character, "Stunned").unwrap();
    game.tap(KeyCode::Q);
    assert_eq!(game.mana(), 40.0);
    assert_eq!(game.target(), 25.0);

    // Not enough Mana.
    game.world
        .set_attribute(game.character, "Mana", Some(Value::Number(10.0)))
        .unwrap();
    game.seconds(3.1);
    game.tap(KeyCode::Q);
    assert_eq!(game.mana(), 10.0);
    assert_eq!(output(&mut game.runtime), Vec::<String>::new());
}

#[test]
fn can_activate_says_why_not_and_events_fire_with_cooldown_changes() {
    let mut game = Game::new();
    let ability = fireball(&mut game);
    game.world
        .set_property(ability, "Duration", Value::Number(1.0))
        .unwrap();
    game.world
        .set_property(ability, "GrantsTags", Value::String("Casting".to_owned()))
        .unwrap();
    start(
        &mut game.world,
        &mut game.runtime,
        r#"
local fireball = workspace.Character.Fireball
fireball.Activated:Connect(function(player) print("activated", player and player.Name or "nil") end)
fireball.Ended:Connect(function(cancelled) print("ended", cancelled) end)
fireball.CooldownChanged:Connect(function(left) print("cooldown", left) end)
print(fireball:CanActivate())
print(fireball:Activate(), workspace.Character:HasTag("Casting"), fireball.Active)
print(fireball:CanActivate())
print(pcall(function() fireball.CooldownLeft = 0 end))
"#,
    );
    game.runtime.fire_changes(&mut game.world);
    let lines = output(&mut game.runtime);
    assert_eq!(lines[0], "true nil");
    assert_eq!(lines[1], "true true true");
    assert_eq!(lines[2], "false it's cooling down (3.0 s left)");
    assert!(
        lines[3].starts_with("false ") && lines[3].contains("read-only"),
        "{}",
        lines[3]
    );
    assert_eq!(lines[4..], ["activated nil", "cooldown 3"]);

    game.seconds(1.1);
    assert_eq!(output(&mut game.runtime), ["ended false"]);
    assert!(!game.world.get(game.character).unwrap().has_tag("Casting"));
    game.seconds(2.0);
    assert_eq!(output(&mut game.runtime), ["cooldown 0"]);

    // The player's key passes their Player; Cancel ends it early.
    game.tap(KeyCode::Q);
    start(
        &mut game.world,
        &mut game.runtime,
        "workspace.Character.Fireball:Cancel()",
    );
    game.runtime.fire_changes(&mut game.world);
    assert_eq!(
        output(&mut game.runtime),
        ["activated Player", "cooldown 3", "ended true"]
    );
    assert!(!game.world.get(game.character).unwrap().has_tag("Casting"));
}

#[test]
fn a_local_script_asks_the_server_which_activates_for_its_player() {
    let mut game = Game::new();
    fireball(&mut game);
    start_local(
        &mut game.world,
        &mut game.runtime,
        r#"
local fireball = workspace.Character.Fireball
print("asked", fireball:Activate())
"#,
    );
    game.frame(&[]);
    assert_eq!(game.mana(), 80.0);
    assert_eq!(game.target(), 75.0);
    assert_eq!(output(&mut game.runtime), ["asked true"]);
}

/// A Poison template in ReplicatedStorage: 2 damage every second for 10
/// seconds, up to 3 stacks, tagging the Model Poisoned.
fn poison(world: &mut World) -> InstanceId {
    let effect = world.create(ClassName::Effect).unwrap();
    world.set_name(effect, "Poison").unwrap();
    for (name, value) in [
        ("Attribute", Value::String("Health".to_owned())),
        ("Amount", Value::Number(-2.0)),
        ("Duration", Value::Number(10.0)),
        ("Period", Value::Number(1.0)),
        ("MaxStacks", Value::Number(3.0)),
        ("GrantsTags", Value::String("Poisoned".to_owned())),
    ] {
        world.set_property(effect, name, value).unwrap();
    }
    world
        .set_parent(effect, Some(world.replicated_storage()))
        .unwrap();
    effect
}

#[test]
fn a_ten_second_poison_damages_every_second_stacks_to_three_and_stops_once_removed() {
    let mut game = Game::new();
    poison(&mut game.world);
    let health = game.target_health;
    start(
        &mut game.world,
        &mut game.runtime,
        r#"
local poison = game.ReplicatedStorage.Poison
local target = workspace.Target
local applied
for _ = 1, 4 do
    applied = target:ApplyEffect(poison)
end
print(applied.Stacks, applied.Parent.Name, target:HasTag("Poisoned"), #target:GetChildren())
"#,
    );
    game.runtime.fire_changes(&mut game.world);
    assert_eq!(output(&mut game.runtime), ["3 Target true 3"]);
    assert_eq!(
        number(&game.world, health, "Health"),
        100.0,
        "nothing at once"
    );

    game.seconds(2.0);
    assert_eq!(
        number(&game.world, health, "Health"),
        88.0,
        "2 × 3 stacks, twice"
    );

    // Removed: no more damage, the tag goes, and what it did stays done.
    let effect = game
        .world
        .find_first_child(game.runtime_target(), "Poison")
        .unwrap();
    game.world.destroy(effect).unwrap();
    game.seconds(3.0);
    assert_eq!(number(&game.world, health, "Health"), 88.0);
    let target = game.runtime_target();
    assert!(!game.world.get(target).unwrap().has_tag("Poisoned"));

    // Left alone, one stack does 10 hits of 2 and is gone after 10 s.
    start(
        &mut game.world,
        &mut game.runtime,
        "workspace.Target:ApplyEffect(game.ReplicatedStorage.Poison)",
    );
    game.seconds(5.0);
    let time_left = number(
        &game.world,
        game.world.find_first_child(target, "Poison").unwrap(),
        "TimeLeft",
    );
    assert!((time_left - 5.0).abs() < 0.05, "{time_left}");
    game.seconds(6.0);
    assert_eq!(number(&game.world, health, "Health"), 68.0);
    assert!(game.world.find_first_child(target, "Poison").is_none());
    assert!(!game.world.get(target).unwrap().has_tag("Poisoned"));
}

impl Game {
    fn runtime_target(&self) -> InstanceId {
        self.world
            .find_first_child(self.world.workspace(), "Target")
            .unwrap()
    }
}

#[test]
fn a_lasting_effect_is_undone_when_removed_and_an_instant_one_changes_once() {
    let mut game = Game::new();
    start(
        &mut game.world,
        &mut game.runtime,
        r#"
local function effect(name, attribute, operation, amount, duration)
    local e = Instance.new("Effect")
    e.Name = name
    e.Attribute = attribute
    e.Operation = operation
    e.Amount = amount
    e.Duration = duration
    e.Parent = game.ReplicatedStorage
    return e
end
local target = workspace.Target
local tough = target:ApplyEffect(effect("Tough", "MaxHealth", Enum.EffectOperation.Add, 50, -1))
local haste = target:ApplyEffect(effect("Haste", "WalkSpeed", Enum.EffectOperation.Multiply, 1.5, 4))
target:SetAttribute("Gold", 5)
local gift = target:ApplyEffect(effect("Gift", "Gold", "Add", 10, 0))
print(target.Health.MaxHealth, target.ThirdPersonController.WalkSpeed, target:GetAttribute("Gold"), gift)
tough:Destroy()
"#,
    );
    game.runtime.fire_changes(&mut game.world);
    let target = game.runtime_target();
    let controller = game
        .world
        .find_first_child(target, "ThirdPersonController")
        .unwrap();
    let walk = number(&game.world, controller, "WalkSpeed");
    assert_eq!(output(&mut game.runtime), [format!("150 {walk} 15 nil")]);
    game.frame(&[]);
    assert_eq!(number(&game.world, game.target_health, "MaxHealth"), 100.0);
    game.seconds(4.1);
    let back = number(&game.world, controller, "WalkSpeed");
    assert!((back * 1.5 - walk).abs() < 1e-9, "{back} then {walk}");
    assert_eq!(attribute(&game.world, target, "Gold"), 15.0);
}

#[test]
fn abilities_and_effects_are_made_from_data_table_rows() {
    let mut table = DataTable::default();
    table.add_column("Cooldown", ColumnType::Number).unwrap();
    table
        .add_column("CostAttribute", ColumnType::String)
        .unwrap();
    table.add_column("CostAmount", ColumnType::Integer).unwrap();
    table.add_column("Damage", ColumnType::Number).unwrap();
    let operation = ColumnType::Enum {
        options: vec!["Add".into(), "Multiply".into()],
    };
    table.add_column("Operation", operation).unwrap();
    let row = table.add_row("Fireball", None);
    table.set_cell(&row, "Cooldown", Cell::Number(3.0)).unwrap();
    let mana = Cell::Text("Mana".into());
    table.set_cell(&row, "CostAttribute", mana).unwrap();
    table
        .set_cell(&row, "CostAmount", Cell::Integer(20))
        .unwrap();
    table.set_cell(&row, "Damage", Cell::Number(25.0)).unwrap();
    let row = table.add_row("Slow", None);
    let multiply = Cell::Text("Multiply".into());
    table.set_cell(&row, "Operation", multiply).unwrap();
    let assets = Assets::new(std::env::temp_dir());
    assets.put_data_table("assets/data/spells.data.json", table);

    let mut world = World::new();
    let mut runtime = script_runtime();
    runtime.set_assets(assets);
    start(
        &mut world,
        &mut runtime,
        r#"
local DataService = game:GetService("DataService")
local fireball = DataService:CreateAbility("spells", "Fireball")
print(fireball.ClassName, fireball.Name, fireball.Cooldown, fireball.CostAttribute, fireball.CostAmount, fireball:GetAttribute("Damage"), fireball.Parent)
local slow = DataService:CreateEffect("spells", "Slow")
print(slow.ClassName, slow.Operation == Enum.EffectOperation.Multiply)
print(pcall(function() return DataService:CreateAbility("spells", "Nope") end))
"#,
    );
    let lines = output(&mut runtime);
    assert_eq!(lines[0], "Ability Fireball 3 Mana 20 25 nil");
    assert_eq!(lines[1], "Effect true");
    assert!(lines[2].contains("has no row \"Nope\""), "{}", lines[2]);
}
