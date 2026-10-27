//! Data tables in the editor (unified Phase 33): making a table in the
//! Data Table tab, a script reading it in Play, and a cell changed during
//! Play reaching the running game.

use axcenity_assets::{AssetType, Cell, ColumnType};
use axcenity_core::Value;

use super::tests::{copy_of_example, play_for, printed};
use super::*;
use crate::data_table::{TableAction, TableEdit};

const SIZE: egui::Vec2 = egui::vec2(1200.0, 700.0);
const WEAPONS: &str = "assets/data/weapons.data.json";

/// Draws the Data Table tab a few times.
fn draw(app: &mut EditorApp, ctx: &egui::Context, name: &str) {
    let mut draw = |ctx: &egui::Context| {
        egui::CentralPanel::default().show(ctx, |ui| app.data_table_pane(ui));
    };
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
            ..Default::default()
        };
        let _ = ctx.run(input, &mut draw);
    }
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        crate::snapshot::snapshot(
            &std::path::Path::new(&folder).join(format!("data-table-{name}.png")),
            SIZE,
            vec![Vec::new(); 3],
            &mut draw,
        );
    }
}

/// The output, with the tabs `print` puts between values as spaces.
fn lines(app: &EditorApp) -> Vec<String> {
    printed(app)
        .iter()
        .map(|line| line.replace('\t', " "))
        .collect()
}

fn set(app: &mut EditorApp, row: &str, column: &str, cell: Cell) {
    app.data_tables
        .edit(TableEdit::SetCell {
            row: row.to_owned(),
            column: column.to_owned(),
            cell,
        })
        .unwrap();
    app.data_tables.finish_edit();
}

fn size_y(app: &EditorApp, name: &str) -> f32 {
    let world = shown(&app.world, &app.play);
    let part = world.find_first_child(world.workspace(), name).unwrap();
    match world.get(part).unwrap().property("Size") {
        Some(Value::Vector3(size)) => size.y,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_weapons_table_made_in_the_grid_is_read_by_a_script() {
    let folder = copy_of_example("hello-data", "data-made");
    // Start without the example's table and script, to make them here.
    std::fs::remove_file(folder.join(WEAPONS)).unwrap();
    std::fs::write(
        folder.join("scripts/armory.server.lua"),
        "print(\"sword does\", game:GetService(\"DataService\"):GetTable(\"weapons\"):Get(\"sword\").Damage)\n",
    )
    .unwrap();
    let mut app = EditorApp::new(Some(&folder));
    let ctx = theme::test_context();

    // New table, then its columns and a row, as the grid's menus ask.
    app.data_table_action(TableAction::New("weapons".to_owned()));
    draw(&mut app, &ctx, "new");
    assert_eq!(app.data_tables.current(), Some(WEAPONS));
    for (name, kind) in [
        ("Damage", ColumnType::Number),
        ("Range", ColumnType::Integer),
        (
            "Icon",
            ColumnType::Asset {
                asset: AssetType::Texture,
            },
        ),
    ] {
        app.data_tables
            .edit(TableEdit::AddColumn {
                name: name.to_owned(),
                kind,
            })
            .unwrap();
    }
    app.data_tables.edit(TableEdit::AddRow).unwrap();
    app.data_tables
        .edit(TableEdit::RenameRow {
            old: "row".to_owned(),
            new: "sword".to_owned(),
        })
        .unwrap();
    set(&mut app, "sword", "Damage", Cell::Number(12.5));
    set(&mut app, "sword", "Range", Cell::Integer(2));
    let icon = Cell::Text("assets/textures/sword.png".to_owned());
    set(&mut app, "sword", "Icon", icon);
    draw(&mut app, &ctx, "weapons");

    // Each edit is one undo step.
    let table = |app: &EditorApp| app.data_tables.document(WEAPONS).unwrap().table.clone();
    let made = table(&app);
    assert!(app.data_tables.undo());
    assert_eq!(
        table(&app).row("sword").unwrap().cells.get("Icon"),
        None,
        "undo takes back only the Icon"
    );
    assert!(app.data_tables.redo());
    assert_eq!(table(&app), made);

    // Saved with the project, then read by the script in Play.
    assert!(app.data_tables.unsaved());
    assert!(app.save());
    assert!(!app.data_tables.unsaved());
    let written = axcenity_assets::read_data_table(&folder.join(WEAPONS)).unwrap();
    assert_eq!(written, made);
    app.play();
    play_for(&mut app, 0.1);
    assert!(
        lines(&app).iter().any(|line| line == "sword does 12.5"),
        "{:?}",
        lines(&app)
    );
    app.stop();
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn changing_a_cell_during_play_changes_what_the_script_reads_next() {
    let folder = copy_of_example("hello-data", "data-live");
    let mut app = EditorApp::new(Some(&folder));
    app.play();
    play_for(&mut app, 0.2);
    let output = lines(&app);
    assert!(
        output
            .iter()
            .any(|line| line == "sword Melee 10 damage, no ammo"),
        "{output:?}"
    );
    assert!(
        output
            .iter()
            .any(|line| line == "bow Ranged 6 damage, 20 arrows"),
        "{output:?}"
    );
    assert_eq!(size_y(&app, "sword"), 3.0);

    // The table's Damage is changed in the grid while the game runs.
    app.data_table_action(TableAction::Show(WEAPONS.to_owned()));
    let ctx = theme::test_context();
    draw(&mut app, &ctx, "play");
    set(&mut app, "sword", "Damage", Cell::Number(20.0));
    draw(&mut app, &ctx, "play-edited");
    play_for(&mut app, 1.1);
    assert!(app.playing(), "the game wasn't restarted");
    assert_eq!(size_y(&app, "sword"), 5.0);
    app.stop();
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn search_filter_and_sort_change_only_what_is_shown() {
    let folder = copy_of_example("hello-data", "data-view");
    let mut app = EditorApp::new(Some(&folder));
    app.data_table_action(TableAction::Show(WEAPONS.to_owned()));
    let table = app.data_tables.document(WEAPONS).unwrap().table.clone();
    let keys = |app: &EditorApp| -> Vec<String> {
        app.data_tables
            .shown_rows(&table)
            .into_iter()
            .map(|index| table.rows[index].key.clone())
            .collect()
    };
    assert_eq!(keys(&app), ["sword", "bow", "crossbow"]);
    app.data_tables.sort_by(Some("Damage"), false);
    assert_eq!(keys(&app), ["crossbow", "sword", "bow"]);
    app.data_tables.set_view("bow", None);
    assert_eq!(keys(&app), ["crossbow", "bow"]);
    app.data_tables.set_view("", Some(("Kind", "melee")));
    assert_eq!(keys(&app), ["sword"]);
    assert!(!app.data_tables.unsaved());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_missing_row_or_asset_in_a_table_is_in_asset_check_and_its_fixes_work() {
    use axcenity_project::{self as project, FindingKind, Site};

    use crate::asset_check::FixChoice;

    let folder = copy_of_example("hello-data", "data-check");
    // The bow's icon moved, and the crossbow names ammo there isn't.
    std::fs::create_dir_all(folder.join("assets/textures/moved")).unwrap();
    std::fs::rename(
        folder.join("assets/textures/bow.png"),
        folder.join("assets/textures/moved/bow.png"),
    )
    .unwrap();
    let path = folder.join(WEAPONS);
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.replace("\"bolts\"", "\"darts\"")).unwrap();

    let mut app = EditorApp::new(Some(&folder));
    app.data_table_action(TableAction::Show(WEAPONS.to_owned()));
    app.run_asset_check();
    let findings: Vec<_> = app.asset_check.shown().into_iter().cloned().collect();
    let cell = |row: &str, column: &str| Site::TableCell {
        file: WEAPONS.to_owned(),
        row: row.to_owned(),
        column: column.to_owned(),
    };
    let icon = findings
        .iter()
        .find(|finding| finding.site == cell("bow", "Icon"))
        .unwrap_or_else(|| panic!("the bow's icon: {findings:?}"))
        .clone();
    assert_eq!(icon.kind, FindingKind::BrokenDependency);
    assert_eq!(icon.path, "assets/textures/bow.png");
    let ammo = findings
        .iter()
        .find(|finding| finding.site == cell("crossbow", "Ammo"))
        .unwrap_or_else(|| panic!("the crossbow's ammo: {findings:?}"))
        .clone();
    assert_eq!(ammo.kind, FindingKind::BrokenDependency);
    assert_eq!(
        ammo.message,
        "names the row `darts` of `ammo`, which isn't there"
    );
    assert!(ammo.can_clear() && !ammo.can_point_elsewhere());
    assert_eq!(findings.len(), 2, "{findings:?}");

    // Use the moved icon, and clear the missing ammo.
    let files = project::list_project_files(&folder);
    let moved = project::suggestion(&icon, &files).expect("the moved icon");
    assert_eq!(moved, "assets/textures/moved/bow.png");
    app.fix_findings(std::slice::from_ref(&icon), FixChoice::Use(moved.clone()));
    app.fix_findings(std::slice::from_ref(&ammo), FixChoice::Clear);
    assert!(
        app.asset_check.shown().is_empty(),
        "{:?}",
        app.asset_check.shown()
    );
    let fixed = axcenity_assets::read_data_table(&path).unwrap();
    let bow = fixed.row("bow").unwrap();
    assert_eq!(bow.cells["Icon"], Cell::Text(moved.clone()));
    assert_eq!(
        fixed.row("crossbow").unwrap().cells["Ammo"],
        Cell::Text(String::new())
    );
    // The open tab shows the fixed table, and scripts read it.
    assert_eq!(app.data_tables.document(WEAPONS).unwrap().table, fixed);
    let read = app.assets.data_table("weapons").unwrap();
    assert_eq!(read.row("bow").unwrap().cells["Icon"], Cell::Text(moved));

    // Each fix is an undo step of the project.
    app.undo();
    let undone = axcenity_assets::read_data_table(&path).unwrap();
    assert_eq!(
        undone.row("crossbow").unwrap().cells["Ammo"],
        Cell::Text("darts".to_owned())
    );
    let _ = std::fs::remove_dir_all(&folder);
}
