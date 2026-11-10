use super::*;

#[test]
fn server_palettes_leave_out_client_only_blocks() {
    for category in Category::ALL {
        for item in palette(category, false, &[], &[]) {
            match item {
                PaletteItem::Block(kind) => assert!(!kind.client_only(), "{kind:?}"),
                PaletteItem::Reporter(expression) => {
                    assert!(!expression.client_only(), "{expression:?}");
                }
                PaletteItem::NewVariable => {}
            }
        }
    }
    let client: Vec<_> = palette(Category::Events, true, &[], &[])
        .into_iter()
        .filter_map(|item| match item {
            PaletteItem::Block(kind) => Some(*kind),
            _ => None,
        })
        .collect();
    assert!(client.iter().any(BlockKind::client_only));
}

#[test]
fn palette_blocks_name_the_first_object_of_their_kind() {
    let objects = [
        ObjectInfo {
            path: "Floor".to_owned(),
            class: ClassName::Part,
            names: Vec::new(),
        },
        ObjectInfo {
            path: "Floor.Chime".to_owned(),
            class: ClassName::Sound,
            names: Vec::new(),
        },
    ];
    let sound = palette(Category::Sound, false, &objects, &[]);
    assert!(matches!(
        &sound[0],
        PaletteItem::Block(kind) if matches!(&**kind, BlockKind::PlaySound { target } if target == "Floor.Chime")
    ));
}

#[test]
fn slots_take_only_reporters_of_their_shape() {
    assert!(!SlotKind::Condition.accepts(BlockShape::Value));
    assert!(SlotKind::Condition.accepts(BlockShape::Boolean));
    assert!(SlotKind::Number.accepts(BlockShape::Boolean));
    assert!(!SlotKind::Number.accepts(BlockShape::Statement));
}

#[test]
fn dropping_a_reporter_and_stacks_puts_them_where_aimed() {
    let mut script = BlockScript {
        blocks: vec![Block::new(BlockKind::WhenStarted {
            body: vec![Block::new(BlockKind::Wait {
                seconds: Expression::Number { value: 1.0 },
            })],
        })],
        ..Default::default()
    };
    let wait = script.blocks[0].kind.bodies()[0][0].id;
    let address = SlotAddress {
        block: wait,
        path: vec![0],
    };
    *address.resolve(&mut script).unwrap() = Expression::Random {
        min: Box::new(Expression::Number { value: 1.0 }),
        max: Box::new(Expression::Number { value: 3.0 }),
    };
    *address.child(1).resolve(&mut script).unwrap() = Expression::Number { value: 9.0 };
    let source = translate(&script).unwrap().source;
    assert!(
        source.contains("task.wait(math.random(math.floor(1), math.floor(9)))"),
        "{source}"
    );

    let hat = script.blocks[0].id;
    drop_stack(
        &mut script,
        StackSpot::Mouth {
            block: hat,
            mouth: 0,
            empty: false,
        },
        vec![Block::new(BlockKind::StopScript)],
        0.0,
    );
    drop_stack(
        &mut script,
        StackSpot::After {
            block: wait,
            last: true,
        },
        vec![Block::new(BlockKind::Print {
            value: Expression::Text {
                value: "done".to_owned(),
            },
        })],
        0.0,
    );
    let source = translate(&script).unwrap().source;
    let order: Vec<_> = ["do return end", "task.wait", "print(\"done\")"]
        .iter()
        .map(|code| source.find(code).unwrap())
        .collect();
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{source}");
}

/// A script with every palette block and reporter in it.
pub(super) fn sample_script(objects: &[ObjectInfo]) -> BlockScript {
    let mut events = Vec::new();
    let mut statements = Vec::new();
    let mut reporters = Vec::new();
    for category in Category::ALL {
        for item in palette(category, true, objects, &["score".to_owned()]) {
            match item {
                PaletteItem::Block(kind) if kind.is_event() => events.push(Block::new(*kind)),
                PaletteItem::Block(kind) => statements.push(Block::new(*kind)),
                PaletteItem::Reporter(expression) => reporters.push(expression),
                PaletteItem::NewVariable => {}
            }
        }
    }
    let mut blocks = Vec::new();
    let mut first = events.remove(0);
    first.kind.bodies_mut()[0].extend(statements);
    blocks.push(first);
    let mut touched = events.remove(0);
    touched.position = Some([520.0, 20.0]);
    touched.kind.bodies_mut()[0].extend(reporters.into_iter().map(|value| {
        if value.shape() == BlockShape::Boolean {
            Block::new(BlockKind::If {
                condition: value,
                body: vec![Block::new(BlockKind::StopScript)],
            })
        } else {
            Block::new(BlockKind::Print { value })
        }
    }));
    blocks.push(touched);
    blocks.push(Block {
        position: Some([520.0, 700.0]),
        ..Block::new(BlockKind::Unattached {
            body: vec![Block::new(BlockKind::Wait {
                seconds: Expression::Number { value: 2.5 },
            })],
        })
    });
    // Lua with no blocks, from a script switched from Code.
    blocks.push(Block {
        position: Some([800.0, 20.0]),
        ..Block::new(BlockKind::TopCode {
            code: "local function double(x)
return x * 2
end"
            .to_owned(),
        })
    });
    blocks.push(Block {
        position: Some([800.0, 180.0]),
        ..Block::new(BlockKind::WhenStarted {
            body: vec![
                Block::new(BlockKind::Code {
                    code: "for i = 1, 3 do
print(i)
end"
                    .to_owned(),
                }),
                Block::new(BlockKind::Print {
                    value: Expression::Code {
                        code: "double(4)".to_owned(),
                    },
                }),
            ],
        })
    });
    BlockScript {
        blocks,
        ..Default::default()
    }
}

#[test]
fn every_block_draws_in_both_themes_without_changing_the_script() {
    let objects = vec![
        ObjectInfo {
            path: "Pad".to_owned(),
            class: ClassName::Part,
            names: Vec::new(),
        },
        ObjectInfo {
            path: "Pad.Chime".to_owned(),
            class: ClassName::Sound,
            names: Vec::new(),
        },
    ];
    let script = sample_script(&objects);
    let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(std::path::PathBuf::from);
    for (theme, name) in [(egui::Theme::Dark, "dark"), (egui::Theme::Light, "light")] {
        let mut drawn = script.clone();
        let mut show_all = |ctx: &egui::Context| {
            ctx.set_theme(theme);
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    show(
                        ui,
                        &mut drawn,
                        BlockContext {
                            key: "test",
                            client: true,
                            highlighted: None,
                            objects: &objects,
                        },
                    );
                });
        };
        match &snapshots {
            Some(folder) => crate::snapshot::snapshot(
                &folder.join(format!("blocks-{name}.png")),
                vec2(1500.0, 950.0),
                vec![Vec::new(); 3],
                &mut show_all,
            ),
            None => {
                let ctx = theme::test_context();
                for _ in 0..3 {
                    let input = egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1500.0, 950.0))),
                        ..Default::default()
                    };
                    let _ = ctx.run(input, &mut show_all);
                }
            }
        }
        assert_eq!(drawn, script, "drawing alone changed the script");
    }
}

/// The frames of dragging with the left mouse button from `from` to
/// `to`, in a few steps, then letting go.
pub(super) fn drag_frames(from: Pos2, to: Pos2) -> Vec<Vec<egui::Event>> {
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let mut frames = vec![
        vec![egui::Event::PointerMoved(from)],
        vec![button(from, true)],
    ];
    for step in 1..=4 {
        frames.push(vec![egui::Event::PointerMoved(
            from.lerp(to, step as f32 / 4.0),
        )]);
    }
    frames.push(vec![egui::Event::PointerMoved(to)]);
    frames.push(vec![button(to, false)]);
    frames.push(Vec::new());
    frames
}

/// Runs `frames` of the editor on `script`, saving the frame before
/// the button is let go as `name` when snapshots are asked for.
fn run_editor(script: &mut BlockScript, frames: Vec<Vec<egui::Event>>, name: &str) {
    let size = vec2(1500.0, 950.0);
    let editor = |script: &mut BlockScript, ctx: &egui::Context| {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                show(
                    ui,
                    script,
                    BlockContext {
                        key: "drag test",
                        client: true,
                        highlighted: None,
                        objects: &[],
                    },
                );
            });
    };
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        // The same frames from the same start, stopping while the
        // block is still held.
        let mut replay = script.clone();
        let held = frames[..frames.len() - 2].to_vec();
        let path = std::path::PathBuf::from(folder).join(format!("{name}.png"));
        crate::snapshot::snapshot(&path, size, held, |ctx| editor(&mut replay, ctx));
    }
    let ctx = theme::test_context();
    // Half a second a frame, as the snapshot replays them, so animations
    // (a palette scrolling to a category) finish.
    for (index, events) in frames.into_iter().enumerate() {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            events,
            time: Some(index as f64 * 0.5),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| editor(script, ctx));
    }
}

#[test]
fn blocks_drag_from_the_palette_and_snap_into_stacks() {
    let mut script = BlockScript::default();
    // "when game starts", the palette's first block (under the search
    // box), onto the canvas.
    run_editor(
        &mut script,
        drag_frames(pos2(150.0, 117.0 + SEARCH_HEIGHT), pos2(700.0, 400.0)),
        "drag-hat",
    );
    assert_eq!(script.blocks.len(), 1, "{script:?}");
    assert!(matches!(
        script.blocks[0].kind,
        BlockKind::WhenStarted { .. }
    ));
    let [x, y] = script.blocks[0]
        .position
        .expect("dropped where it was let go");

    // "wait 1 seconds" (the first Control block) to just under it: it
    // snaps into the event's stack. Clicking Control in the category
    // column first scrolls the palette to it.
    let hat_top_left = pos2(CATEGORY_WIDTH + PALETTE_WIDTH, TOOLBAR_HEIGHT) + vec2(x, y);
    let control = pos2(CATEGORY_WIDTH / 2.0, TOOLBAR_HEIGHT + 8.0 + 56.0 + 27.0);
    let click = |pressed| egui::Event::PointerButton {
        pos: control,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    // Where the palette draws it then (Inter's line height sets the rows).
    let wait_grab = pos2(110.0, 86.0 + SEARCH_HEIGHT);
    let wait_palette_top_left = pos2(88.0, 66.0 + SEARCH_HEIGHT);
    let target =
        hat_top_left + vec2(0.0, CAP + ROW_HEIGHT + 10.0) + (wait_grab - wait_palette_top_left);
    let mut frames = vec![
        vec![egui::Event::PointerMoved(control)],
        vec![click(true)],
        vec![click(false)],
        Vec::new(),
        Vec::new(),
    ];
    frames.extend(drag_frames(wait_grab, target));
    run_editor(&mut script, frames, "drag-wait");
    assert_eq!(script.blocks.len(), 1, "it joined the stack: {script:?}");
    let source = translate(&script).unwrap().source;
    assert_eq!(source, "task.spawn(function()\n    task.wait(1)\nend)\n");
}

#[test]
fn view_as_lua_opens_beside_the_canvas() {
    let mut script = sample_script(&[]);
    script.blocks.truncate(1);
    let click = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let button = pos2(1432.0, 20.0);
    let frames = vec![
        vec![egui::Event::PointerMoved(button)],
        vec![click(button, true)],
        vec![click(button, false)],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ];
    let before = script.clone();
    run_editor(&mut script, frames, "lua-panel");
    assert_eq!(script, before, "showing the Lua changes nothing");
}

#[test]
fn rounded_corners_stay_between_their_edges() {
    let square = [
        (pos2(0.0, 0.0), 4.0),
        (pos2(10.0, 0.0), 4.0),
        (pos2(10.0, 10.0), 4.0),
        (pos2(0.0, 10.0), 4.0),
    ];
    let points = rounded(&square, None, None);
    assert!(points.len() > 4);
    assert!(
        points
            .iter()
            .all(|point| { (0.0..=10.0).contains(&point.x) && (0.0..=10.0).contains(&point.y) })
    );
}

/// Every palette item the search finds for `search`, in a client script.
fn found(search: &str) -> Vec<PaletteItem> {
    let mut items = Vec::new();
    for category in Category::ALL {
        for item in palette(category, true, &[], &[]) {
            if matches_search(&search_text(&item, category, &[]), search) {
                items.push(item);
            }
        }
    }
    items
}

#[test]
fn the_palette_search_finds_blocks_by_their_words_and_their_lua() {
    let blocks = |search| {
        found(search)
            .into_iter()
            .filter_map(|item| match item {
                PaletteItem::Block(kind) => Some(*kind),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let everything = found("").len();
    assert!(everything > 100, "an empty search shows the whole palette");
    // By the words on the block, in any case and order.
    assert!(
        blocks("PLAY sound")
            .iter()
            .any(|kind| matches!(kind, BlockKind::PlaySound { .. }))
    );
    assert!(
        blocks("touched when")
            .iter()
            .any(|kind| matches!(kind, BlockKind::WhenTouched { .. }))
    );
    // By the Lua it becomes, for someone who knows the Roblox-style name.
    assert!(
        blocks("Destroy")
            .iter()
            .any(|kind| matches!(kind, BlockKind::Destroy { .. }))
    );
    assert!(
        found("Raycast")
            .iter()
            .any(|item| matches!(item, PaletteItem::Reporter(Expression::Raycast { .. })))
    );
    // A narrower search finds fewer, and nonsense finds nothing.
    assert!(found("sound").len() < everything);
    assert!(found("zzzz qqqq").is_empty());
}

#[test]
fn searching_the_palette_shows_the_matches_and_they_drag_out() {
    let mut script = BlockScript::default();
    let search_box = pos2(CATEGORY_WIDTH + 120.0, TOOLBAR_HEIGHT + SEARCH_HEIGHT / 2.0);
    let click = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let mut frames = vec![
        vec![egui::Event::PointerMoved(search_box)],
        vec![click(search_box, true)],
        vec![click(search_box, false)],
        vec![egui::Event::Text("wait until".to_owned())],
        Vec::new(),
    ];
    // The only match, "wait until", is first under its category's heading,
    // where "when game starts" is with no search.
    frames.extend(drag_frames(
        pos2(110.0, 86.0 + SEARCH_HEIGHT),
        pos2(700.0, 400.0),
    ));
    run_editor(&mut script, frames, "palette-search");
    assert_eq!(script.blocks.len(), 1, "{script:?}");
    let BlockKind::Unattached { body } = &script.blocks[0].kind else {
        panic!("a loose stack: {script:?}");
    };
    assert!(
        matches!(body[0].kind, BlockKind::WaitUntil { .. }),
        "{script:?}"
    );
}

#[test]
fn hovering_a_palette_block_says_the_lua_it_becomes() {
    let mut script = BlockScript::default();
    let hat = pos2(150.0, 117.0 + SEARCH_HEIGHT);
    // Held still long enough for the tooltip.
    let frames = vec![
        vec![egui::Event::PointerMoved(hat)],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ];
    let before = script.clone();
    run_editor(&mut script, frames, "palette-hover");
    assert_eq!(script, before, "hovering changes nothing");
    assert_eq!(
        palette::item_lua(&PaletteItem::Block(Box::new(BlockKind::WhenStarted {
            body: Vec::new()
        })))
        .as_deref(),
        Some("task.spawn(function()\nend)")
    );
}

#[test]
fn luau_blocks_fold_under_their_own_heading_but_stay_in_the_graph_menu() {
    let (own, luau) = palette_parts(Category::Variables, true, &[], &[]);
    let kinds = |items: &[PaletteItem]| {
        items
            .iter()
            .filter_map(|item| match item {
                PaletteItem::Block(kind) => Some(kind.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert!(kinds(&own).iter().all(|kind| matches!(
        **kind,
        BlockKind::SetVariable { .. } | BlockKind::ChangeVariable { .. }
    )));
    assert!(
        kinds(&luau)
            .iter()
            .any(|kind| matches!(**kind, BlockKind::Local { .. }))
    );
    // The graph's node menu and searches still offer every block.
    assert_eq!(
        palette(Category::Variables, true, &[], &[]).len(),
        own.len() + luau.len()
    );
    assert!(!found("local").is_empty());

    // In the palette: Variables, then open "Lua blocks", then drag
    // "local name = 0" out.
    let mut script = BlockScript::default();
    let variables = pos2(
        CATEGORY_WIDTH / 2.0,
        TOOLBAR_HEIGHT + 8.0 + 56.0 * 6.0 + 27.0,
    );
    let heading = pos2(120.0, 291.0);
    let click = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let mut frames = Vec::new();
    for at in [variables, heading] {
        frames.extend([
            vec![egui::Event::PointerMoved(at)],
            vec![click(at, true)],
            vec![click(at, false)],
            Vec::new(),
            Vec::new(),
        ]);
    }
    frames.extend(drag_frames(pos2(110.0, 330.0), pos2(700.0, 400.0)));
    run_editor(&mut script, frames, "palette-lua-blocks");
    assert_eq!(script.blocks.len(), 1, "{script:?}");
    let BlockKind::Unattached { body } = &script.blocks[0].kind else {
        panic!("a loose stack: {script:?}");
    };
    assert!(
        matches!(body[0].kind, BlockKind::Local { .. }),
        "{script:?}"
    );
}
