//! Tests for unified Phase 33: `DataService` and read-only data tables.

use std::path::{Path, PathBuf};

use axcenity_assets::{AssetType, Assets, Cell, ColumnType, DataTable, write_data_table};
use axcenity_core::{ClassName, World};

use super::*;

/// A project folder with a `weapons` table (Damage, Range, Icon, Ammo,
/// Tint, Offset, Kind) and an `ammo` table.
fn project(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("axcenity-data-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    write_weapons(&folder, 10.0);
    let mut ammo = DataTable::default();
    ammo.add_column("Count", ColumnType::Integer).unwrap();
    let arrows = ammo.add_row("arrows", None);
    ammo.set_cell(&arrows, "Count", Cell::Integer(20)).unwrap();
    write_data_table(&folder.join("assets/data/ammo.data.json"), &ammo).unwrap();
    folder
}

fn weapons(damage: f64) -> DataTable {
    let mut table = DataTable::default();
    table.add_column("Damage", ColumnType::Number).unwrap();
    table.add_column("Range", ColumnType::Integer).unwrap();
    let icon = ColumnType::Asset {
        asset: AssetType::Texture,
    };
    table.add_column("Icon", icon).unwrap();
    let ammo = ColumnType::Reference {
        table: "ammo".into(),
    };
    table.add_column("Ammo", ammo).unwrap();
    table.add_column("Tint", ColumnType::Color3).unwrap();
    table.add_column("Offset", ColumnType::Vector3).unwrap();
    let kind = ColumnType::Enum {
        options: vec!["Melee".into(), "Ranged".into()],
    };
    table.add_column("Kind", kind).unwrap();
    let sword = table.add_row("sword", None);
    table
        .set_cell(&sword, "Damage", Cell::Number(damage))
        .unwrap();
    table.set_cell(&sword, "Range", Cell::Integer(3)).unwrap();
    let icon = Cell::Text("assets/textures/sword.png".into());
    table.set_cell(&sword, "Icon", icon).unwrap();
    let bow = table.add_row("bow", None);
    table
        .set_cell(&bow, "Ammo", Cell::Text("arrows".into()))
        .unwrap();
    table
        .set_cell(&bow, "Kind", Cell::Text("Ranged".into()))
        .unwrap();
    let offset = Cell::Vector3([0.0, 1.5, 0.0]);
    table.set_cell(&bow, "Offset", offset).unwrap();
    table
}

fn write_weapons(folder: &Path, damage: f64) {
    let path = folder.join("assets/data/weapons.data.json");
    write_data_table(&path, &weapons(damage)).unwrap();
}

/// Starts `source` as a Script with the project's assets.
fn run(folder: &Path, source: &str) -> (ScriptRuntime, World) {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime.set_assets(Assets::new(folder));
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(&mut world, script, "scripts/data.server.lua", source);
    (runtime, world)
}

fn lines(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Error => format!("error: {}", line.text),
            _ => line.text.replace('\t', " "),
        })
        .collect()
}

#[test]
fn a_script_reads_a_rows_columns_by_name() {
    let folder = project("read");
    let (mut runtime, _world) = run(
        &folder,
        r#"
local weapons = game:GetService("DataService"):GetTable("weapons")
print(weapons:Get("sword").Damage, weapons:Get("sword").Range, weapons:Get("sword").Icon)
local bow = weapons:Get("bow")
print(bow.Kind, bow.Offset.Y, typeof(bow.Tint), bow.Tint.R)
print(bow.Ammo.Count, weapons:Get("sword").Ammo, weapons:Get("axe"))
print(table.concat(weapons:Keys(), ","), weapons:GetAll().bow.Kind)
print(typeof(weapons), typeof(bow), tostring(bow))
local columns = {}
for name, value in weapons:Get("sword") do
    table.insert(columns, name)
end
print(table.concat(columns, ","))
print(game.DataService:GetTable("ammo"):Get("arrows").Count)
"#,
    );
    assert_eq!(
        lines(&mut runtime),
        [
            "10 3 assets/textures/sword.png",
            "Ranged 1.5 Color3 1",
            "20 nil nil",
            "sword,bow Ranged",
            "DataTable DataRow weapons.bow",
            "Damage,Range,Icon,Ammo,Tint,Offset,Kind",
            "20",
        ]
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn writing_to_a_row_or_table_errors_and_says_tables_are_read_only() {
    let folder = project("write");
    let (mut runtime, _world) = run(
        &folder,
        r#"
local weapons = game:GetService("DataService"):GetTable("weapons")
local ok, err = pcall(function()
    weapons:Get("sword").Damage = 99
end)
print(ok, err)
print(pcall(function() weapons.sword = {} end))
print(weapons:Get("sword").Damage)
"#,
    );
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 3, "{output:?}");
    assert!(output[0].starts_with("false "), "{output:?}");
    assert!(
        output[0].contains("data tables are read-only"),
        "{output:?}"
    );
    assert!(
        output[0].contains("\"Damage\" of the row \"sword\""),
        "{output:?}"
    );
    assert!(output[1].contains("read-only"), "{output:?}");
    assert_eq!(output[2], "10");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_wrong_table_or_column_name_says_what_there_is() {
    let folder = project("wrong");
    let (mut runtime, _world) = run(
        &folder,
        r#"
print(pcall(function() return game:GetService("DataService"):GetTable("shields") end))
print(pcall(function() return game:GetService("DataService"):GetTable("weapons"):Get("sword").Damge end))
"#,
    );
    let output = lines(&mut runtime);
    assert!(
        output[0].contains("can't read the data table \"shields\""),
        "{output:?}"
    );
    assert!(
        output[0].contains("assets/data/shields.data.json"),
        "{output:?}"
    );
    assert!(
        output[1].contains("has no column \"Damge\"; its columns are Damage, Range"),
        "{output:?}"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_changed_table_is_what_the_script_reads_next() {
    let folder = project("live");
    let (mut runtime, mut world) = run(
        &folder,
        r#"
local weapons = game:GetService("DataService"):GetTable("weapons")
while true do
    print(weapons:Get("sword").Damage)
    task.wait(1)
end
"#,
    );
    assert_eq!(lines(&mut runtime), ["10"]);
    // The editor's grid hands its edit over without saving…
    let assets = runtime.server.lua.app_data_ref::<Assets>().unwrap().clone();
    assets.put_data_table("assets/data/weapons.data.json", weapons(25.0));
    runtime.step(&mut world, 1.1);
    assert_eq!(lines(&mut runtime), ["25"]);
    // …and a file changed on disk is reloaded the same way.
    write_weapons(&folder, 40.0);
    assets.reload("assets/data/weapons.data.json");
    runtime.step(&mut world, 1.1);
    assert_eq!(lines(&mut runtime), ["40"]);
    let _ = std::fs::remove_dir_all(&folder);
}
