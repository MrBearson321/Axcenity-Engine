use super::*;

fn temp_folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "axcenity-versions-{name}-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(folder.join("scripts")).unwrap();
    fs::write(folder.join(WORLD_FILE), "{\"format_version\":1}").unwrap();
    fs::create_dir_all(folder.join("levels")).unwrap();
    fs::write(
        folder.join("levels/Cave.world.json"),
        "{\"format_version\":1}",
    )
    .unwrap();
    fs::write(folder.join("scripts/hello.server.lua"), "print(1)\n").unwrap();
    fs::write(folder.join("ui.css"), "Frame { }\n").unwrap();
    folder
}

#[test]
fn a_save_can_be_listed_and_restored_and_a_repeat_is_skipped() {
    let folder = temp_folder("restore");
    assert!(list_versions(&folder).is_empty());
    assert!(record_version(&folder, "Save").unwrap());
    assert!(!record_version(&folder, "Save again").unwrap());
    let versions = list_versions(&folder);
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].label, "Save");
    assert!(!versions[0].pinned);
    assert!(versions[0].when().contains("UTC"));

    fs::write(
        folder.join(WORLD_FILE),
        "{\"format_version\":1,\"changed\":true}",
    )
    .unwrap();
    fs::write(folder.join("scripts/extra.client.lua"), "print(2)\n").unwrap();
    fs::write(folder.join("levels/Cave.world.json"), "{}").unwrap();
    fs::write(folder.join("levels/Moon.world.json"), "{}").unwrap();
    assert!(record_version(&folder, "Second").unwrap());

    restore_version(&folder, &versions[0].id).unwrap();
    assert_eq!(
        fs::read_to_string(folder.join(WORLD_FILE)).unwrap(),
        "{\"format_version\":1}"
    );
    assert!(!folder.join("scripts/extra.client.lua").exists());
    // Levels come back too, and one made since goes.
    assert_eq!(
        fs::read_to_string(folder.join("levels/Cave.world.json")).unwrap(),
        "{\"format_version\":1}"
    );
    assert!(!folder.join("levels/Moon.world.json").exists());
    assert_eq!(
        fs::read_to_string(folder.join("scripts/hello.server.lua")).unwrap(),
        "print(1)\n"
    );
    assert!(folder.join("ui.css").is_file());
    let _ = fs::remove_dir_all(&folder);
}

fn world_with(n: usize) -> String {
    format!(
        "{{\"format_version\":1,\"workspace\":{{\"children\":[\
         {{\"class\":\"Part\",\"name\":\"Block\",\"properties\":{{\"Position\":[{n},0,0]}}}}]}}}}"
    )
}

#[test]
fn three_pinned_checkpoints_survive_fifty_later_saves_and_a_restore_keeps_what_it_replaced() {
    let folder = temp_folder("pins");
    let mut checkpoints = Vec::new();
    for (n, message) in ["First room", "Second room", "Boss"]
        .into_iter()
        .enumerate()
    {
        fs::write(folder.join(WORLD_FILE), world_with(1000 + n)).unwrap();
        checkpoints.push(record_checkpoint(&folder, message).unwrap());
    }
    for n in 0..50 {
        fs::write(folder.join(WORLD_FILE), world_with(n)).unwrap();
        assert!(record_version(&folder, "Save").unwrap(), "save {n}");
    }
    let versions = list_versions(&folder);
    let pinned: Vec<&str> = versions
        .iter()
        .filter(|version| version.pinned)
        .map(|version| version.label.as_str())
        .collect();
    assert_eq!(pinned, ["Boss", "Second room", "First room"]);
    for id in &checkpoints {
        assert!(versions.iter().any(|version| &version.id == id));
    }
    assert_eq!(versions.len(), 3 + MAX_VERSIONS);
    // Newest first, even with all of them in the same second.
    assert_eq!(
        fs::read_to_string(
            folder
                .join(HISTORY_DIR)
                .join(&versions[0].id)
                .join(WORLD_FILE)
        )
        .unwrap(),
        world_with(49)
    );

    restore_version(&folder, &checkpoints[0]).unwrap();
    assert_eq!(
        fs::read_to_string(folder.join(WORLD_FILE)).unwrap(),
        world_with(1000)
    );
    let before = &list_versions(&folder)[0];
    assert_eq!(before.label, "Before restore");
    assert!(before.pinned, "a restore can always be undone");
    assert_eq!(
        fs::read_to_string(folder.join(HISTORY_DIR).join(&before.id).join(WORLD_FILE)).unwrap(),
        world_with(49)
    );
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_checkpoint_of_unchanged_files_pins_the_newest_save_and_unpinning_lets_it_go() {
    let folder = temp_folder("checkpoint");
    record_version(&folder, "Save").unwrap();
    let id = record_checkpoint(&folder, "  Before the boss\nfight ").unwrap();
    let versions = list_versions(&folder);
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].id, id);
    assert_eq!(versions[0].label, "Before the boss fight");
    assert!(versions[0].pinned);
    let unnamed = {
        fs::write(folder.join("ui.css"), "Frame { color: red }\n").unwrap();
        record_checkpoint(&folder, "").unwrap()
    };
    assert_eq!(list_versions(&folder)[0].label, "Checkpoint");

    set_pinned(&folder, &id, false).unwrap();
    assert!(!list_versions(&folder)[1].pinned);
    for n in 0..MAX_VERSIONS {
        fs::write(folder.join(WORLD_FILE), world_with(n)).unwrap();
        record_version(&folder, "Save").unwrap();
    }
    let versions = list_versions(&folder);
    assert!(!versions.iter().any(|version| version.id == id));
    assert!(versions.iter().any(|version| version.id == unnamed));
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn one_file_can_be_restored_and_a_file_made_since_is_removed() {
    let folder = temp_folder("one-file");
    record_version(&folder, "Save").unwrap();
    let first = list_versions(&folder)[0].id.clone();
    fs::write(folder.join("scripts/hello.server.lua"), "print(2)\n").unwrap();
    fs::write(folder.join("scripts/new.server.lua"), "print(3)\n").unwrap();
    fs::write(folder.join(WORLD_FILE), world_with(5)).unwrap();
    record_version(&folder, "Save").unwrap();

    restore_file(&folder, &first, "scripts/hello.server.lua").unwrap();
    assert_eq!(
        fs::read_to_string(folder.join("scripts/hello.server.lua")).unwrap(),
        "print(1)\n"
    );
    // Only that file: the world stays as it is.
    assert_eq!(
        fs::read_to_string(folder.join(WORLD_FILE)).unwrap(),
        world_with(5)
    );
    restore_file(&folder, &first, "scripts/new.server.lua").unwrap();
    assert!(!folder.join("scripts/new.server.lua").exists());
    assert!(restore_file(&folder, &first, "../outside.lua").is_err());
    assert!(restore_file(&folder, &first, "scripts/never.server.lua").is_err());
    assert!(list_versions(&folder)[0].pinned, "kept before the restore");
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn the_change_view_names_a_moved_part_and_an_edited_script() {
    let folder = temp_folder("changes");
    let world = |x: i32, wheels: usize, lamp: bool| {
        let wheels = vec![r#"{ "class": "Part", "name": "Wheel" }"#; wheels].join(
            ",
        ",
        );
        let lamp = if lamp {
            r#"{ "class": "PointLight", "name": "Lamp" },"#
        } else {
            ""
        };
        format!(
            r#"{{
  "format_version": 1,
  "workspace": {{
    "children": [
      {lamp}
      {{ "class": "Part", "name": "Block", "properties": {{ "Position": [{x}, 0, 0], "Color": [1, 0, 0] }} }},
      {{ "class": "Part", "name": "Wall", "properties": {{ "Size": [4, 8, 1] }} }},
      {{ "class": "Model", "name": "Car", "children": [
        {wheels}
      ] }}
    ]
  }},
  "lighting": {{ "properties": {{ "ClockTime": 9 }} }}
}}"#
        )
    };
    fs::write(folder.join(WORLD_FILE), world(0, 2, false)).unwrap();
    fs::write(
        folder.join("scripts/door.server.lua"),
        "local door = workspace.Door\nprint(\"closed\")\nreturn door\n",
    )
    .unwrap();
    fs::create_dir_all(folder.join("assets/images")).unwrap();
    fs::write(folder.join("assets/images/logo.png"), [1, 2, 3]).unwrap();
    record_version(&folder, "Save").unwrap();
    let first = list_versions(&folder)[0].id.clone();

    // Move the Block, edit the door script, take a Wheel off, add a Lamp,
    // and add an image.
    let moved = world(12, 1, true);
    fs::write(folder.join(WORLD_FILE), &moved).unwrap();
    fs::write(
        folder.join("scripts/door.server.lua"),
        "local door = workspace.Door\r\nprint(\"open\")\r\nreturn door\r\n",
    )
    .unwrap();
    fs::write(folder.join("assets/images/sky.png"), [4]).unwrap();
    record_version(&folder, "Save").unwrap();
    let second = list_versions(&folder)[0].id.clone();

    let changes = version_changes(&folder, &second).unwrap();
    let sentences = changes.sentences();
    assert!(
        sentences.contains(&"Moved Part \"Block\" in Workspace".to_owned()),
        "{sentences:#?}"
    );
    assert!(
        sentences.contains(&"Edited scripts/door.server.lua".to_owned()),
        "{sentences:#?}"
    );
    assert!(sentences.contains(&"Removed Part \"Wheel\" from Workspace/Car".to_owned()));
    assert!(sentences.contains(&"Added PointLight \"Lamp\" to Workspace".to_owned()));
    assert!(sentences.contains(&"Added asset assets/images/sky.png".to_owned()));
    // Nothing else: the Wall, the first Wheel and Lighting didn't change,
    // nor did the stylesheet.
    assert_eq!(sentences.len(), 5, "{sentences:#?}");

    let script = changes
        .files
        .iter()
        .find(|file| file.path == "scripts/door.server.lua")
        .unwrap();
    assert_eq!(script.change, Change::Changed);
    assert_eq!(
        script.lines,
        [
            Line::Same("local door = workspace.Door".into()),
            Line::Removed("print(\"closed\")".into()),
            Line::Added("print(\"open\")".into()),
            Line::Same("return door".into()),
        ]
    );
    let shown = around_changes(&script.lines, 0);
    assert_eq!(shown.len(), 4, "a gap, the two lines, a gap");
    assert!(shown[0].is_none() && shown[3].is_none());

    // The oldest version: everything in it is new.
    let oldest = version_changes(&folder, &first).unwrap().sentences();
    assert!(oldest.contains(&"Added Part \"Block\" to Workspace".to_owned()));
    assert!(oldest.contains(&"Added Part \"Wheel\" to Workspace/Car".to_owned()));
    assert!(oldest.contains(&"Changed Lighting: ClockTime".to_owned()));
    assert!(oldest.contains(&"Added scripts/door.server.lua".to_owned()));
    assert!(oldest.contains(&"Added asset assets/images/logo.png".to_owned()));
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn instance_sentences_name_what_changed() {
    let change = |properties: &[&str]| InstanceChange {
        change: Change::Changed,
        class: "Part".into(),
        name: "Door".into(),
        parent: "Workspace/House".into(),
        properties: properties.iter().map(|p| (*p).to_owned()).collect(),
    };
    assert_eq!(
        change(&["Orientation", "Position"]).describe(),
        "Moved Part \"Door\" in Workspace/House"
    );
    assert_eq!(
        change(&["Size"]).describe(),
        "Resized Part \"Door\" in Workspace/House"
    );
    assert_eq!(
        change(&[
            "Color",
            "Material",
            "Position",
            "Size",
            "Transparency",
            "tags"
        ])
        .describe(),
        "Changed Part \"Door\" in Workspace/House: Color, Material, Position, Size and 2 more"
    );
}

#[test]
fn line_diffs_keep_order_and_context() {
    let lines = changes::compare(
        &changes::Side {
            files: vec![("a.lua".into(), "1\n2\n3\n4\n5\n6\n7\n8\n".into())],
            assets: None,
        },
        &changes::Side {
            files: vec![("a.lua".into(), "1\n2\nthree\n4\n5\n6\n7\n8\nnine\n".into())],
            assets: None,
        },
    )
    .files
    .remove(0)
    .lines;
    let shown: Vec<String> = around_changes(&lines, 1)
        .into_iter()
        .map(|line| match line {
            None => "…".to_owned(),
            Some(Line::Same(text)) => format!(" {text}"),
            Some(Line::Added(text)) => format!("+{text}"),
            Some(Line::Removed(text)) => format!("-{text}"),
        })
        .collect();
    assert_eq!(shown, ["…", " 2", "-3", "+three", " 4", "…", " 8", "+nine"]);
}

#[test]
fn the_code_of_scripts_inside_objects_is_kept_and_restored() {
    let folder = temp_folder("objects");
    fs::create_dir_all(folder.join("objects/Door")).unwrap();
    fs::write(folder.join("objects/Door/Open.server.lua"), "-- open\n").unwrap();
    assert!(record_version(&folder, "Save").unwrap());
    let id = list_versions(&folder)[0].id.clone();

    fs::write(folder.join("objects/Door/Open.server.lua"), "-- changed\n").unwrap();
    fs::write(folder.join("objects/Door/New.server.lua"), "-- new\n").unwrap();
    restore_version(&folder, &id).unwrap();
    assert_eq!(
        fs::read_to_string(folder.join("objects/Door/Open.server.lua")).unwrap(),
        "-- open\n"
    );
    assert!(!folder.join("objects/Door/New.server.lua").exists());
    let _ = fs::remove_dir_all(&folder);
}
