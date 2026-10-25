use super::super::tests::{drag_frames, sample_script};
use super::*;

fn number(value: f64) -> Expression {
    Expression::Number { value }
}

fn print(text: &str) -> Block {
    Block::new(BlockKind::Print {
        value: Expression::Text {
            value: text.to_owned(),
        },
    })
}

fn loose(blocks: Vec<Block>) -> Block {
    Block::new(BlockKind::Unattached { body: blocks })
}

fn lua(script: &BlockScript) -> String {
    translate(script).unwrap().source
}

#[test]
fn an_exec_wire_moves_the_chain_and_leaves_what_was_there_loose() {
    let wait = Block::new(BlockKind::Wait {
        seconds: number(1.0),
    });
    let hello = print("hello");
    let (wait_id, hello_id) = (wait.id, hello.id);
    let start = Block::new(BlockKind::WhenStarted { body: vec![wait] });
    let start_id = start.id;
    let mut script = BlockScript {
        blocks: vec![start, loose(vec![hello])],
        ..Default::default()
    };

    connect_exec(&mut script, start_id, Out::Body(0), hello_id).unwrap();
    assert_eq!(
        lua(&script),
        "task.spawn(function()\n    print(\"hello\")\nend)\n"
    );
    // The wait that was wired there is loose now: a node that doesn't run.
    assert!(!has_exec_input(&script, wait_id));
    assert_eq!(script.blocks.len(), 2, "{script:#?}");

    // Then wires the wait after the print.
    connect_exec(&mut script, hello_id, Out::Then, wait_id).unwrap();
    assert_eq!(
        lua(&script),
        "task.spawn(function()\n    print(\"hello\")\n    task.wait(1)\nend)\n"
    );
    assert_eq!(script.blocks.len(), 1, "the emptied loose stack is gone");

    // Breaking the print's input leaves both loose, and nothing runs.
    break_exec_in(&mut script, hello_id);
    assert_eq!(lua(&script), "task.spawn(function()\nend)\n");
    assert!(!has_exec_input(&script, hello_id));
    assert!(
        has_exec_input(&script, wait_id),
        "it's still after the print"
    );
}

#[test]
fn a_wire_that_would_loop_back_is_refused() {
    let first = print("a");
    let second = print("b");
    let (first_id, second_id) = (first.id, second.id);
    let mut script = BlockScript {
        blocks: vec![Block::new(BlockKind::WhenStarted {
            body: vec![first, second],
        })],
        ..Default::default()
    };
    let before = script.clone();
    assert!(connect_exec(&mut script, second_id, Out::Then, first_id).is_err());
    assert!(connect_exec(&mut script, first_id, Out::Then, first_id).is_err());
    assert_eq!(script, before);
}

#[test]
fn the_false_pin_gives_a_branch_its_else_and_loses_it_again() {
    let branch = Block::new(BlockKind::If {
        condition: Expression::Boolean { value: true },
        body: vec![print("yes")],
    });
    let no = print("no");
    let (branch_id, no_id) = (branch.id, no.id);
    let mut script = BlockScript {
        blocks: vec![
            Block::new(BlockKind::WhenStarted { body: vec![branch] }),
            loose(vec![no]),
        ],
        ..Default::default()
    };
    connect_exec(&mut script, branch_id, Out::Body(1), no_id).unwrap();
    assert!(matches!(
        find_block(&script, branch_id).unwrap().kind,
        BlockKind::IfElse { .. }
    ));
    assert!(
        lua(&script).contains("else\n        print(\"no\")"),
        "{}",
        lua(&script)
    );
    break_exec_out(&mut script, branch_id, Out::Body(1));
    assert!(matches!(
        find_block(&script, branch_id).unwrap().kind,
        BlockKind::If { .. }
    ));
    assert!(!lua(&script).contains("else"));
}

#[test]
fn value_wires_check_the_input_and_copy_a_node_that_is_already_wired() {
    let branch = Block::new(BlockKind::If {
        condition: Expression::Boolean { value: false },
        body: Vec::new(),
    });
    let wait = Block::new(BlockKind::Wait {
        seconds: Expression::Random {
            min: Box::new(number(1.0)),
            max: Box::new(number(3.0)),
        },
    });
    let other_wait = Block::new(BlockKind::Wait {
        seconds: number(0.0),
    });
    let (branch_id, wait_id, other_id) = (branch.id, wait.id, other_wait.id);
    let compare = LooseValue::new(Expression::Compare {
        op: Comparison::Greater,
        left: Box::new(Expression::Variable {
            name: "score".to_owned(),
        }),
        right: Box::new(number(10.0)),
    });
    let compare_id = compare.id;
    let mut script = BlockScript {
        blocks: vec![Block::new(BlockKind::WhenStarted {
            body: vec![branch, wait, other_wait],
        })],
        graph: Graph {
            loose: vec![compare],
            nodes: BTreeMap::from([(compare_id.to_string(), [5.0, 6.0])]),
            ..Default::default()
        },
    };
    let condition = ValueAt {
        owner: branch_id,
        path: vec![0],
    };
    let random = ValueAt {
        owner: wait_id,
        path: vec![0],
    };

    // A number doesn't fit a condition.
    assert!(connect_value(&mut script, &random, &condition).is_err());
    // A loose comparison moves in, and keeps its place.
    connect_value(&mut script, &ValueAt::loose(compare_id), &condition).unwrap();
    assert!(script.graph.loose.is_empty());
    assert_eq!(script.graph.nodes.get(&condition.key()), Some(&[5.0, 6.0]));
    // A wired value is copied: both waits use a random number.
    connect_value(
        &mut script,
        &random,
        &ValueAt {
            owner: other_id,
            path: vec![0],
        },
    )
    .unwrap();
    let source = lua(&script);
    assert!(source.contains("if score > 10 then"), "{source}");
    assert_eq!(source.matches("math.random").count(), 2, "{source}");

    // Breaking a value wire leaves the node loose and the input typed in.
    break_value(&mut script, &condition);
    assert_eq!(script.graph.loose.len(), 1);
    assert!(lua(&script).contains("if false then"), "{}", lua(&script));
}

#[test]
fn deleting_a_node_leaves_what_it_was_wired_to() {
    let first = print("a");
    let branch = Block::new(BlockKind::If {
        condition: Expression::Not {
            value: Box::new(Expression::Boolean { value: false }),
        },
        body: vec![print("inside")],
    });
    let last = print("c");
    let (branch_id, last_id) = (branch.id, last.id);
    let mut script = BlockScript {
        blocks: vec![Block::new(BlockKind::WhenStarted {
            body: vec![first, branch, last],
        })],
        ..Default::default()
    };
    delete_nodes(&mut script, &[NodeRef::Block(branch_id)]);
    assert_eq!(
        lua(&script),
        "task.spawn(function()\n    print(\"a\")\nend)\n"
    );
    assert!(find_block(&script, branch_id).is_none());
    assert!(
        !has_exec_input(&script, last_id),
        "what came after is loose"
    );
    assert_eq!(script.graph.loose.len(), 1, "the not node stays");
    // The event goes too: its stack stays, loose.
    let event = script.blocks[0].id;
    delete_nodes(&mut script, &[NodeRef::Block(event)]);
    assert_eq!(lua(&script), "");
    assert!(
        script
            .blocks
            .iter()
            .all(|block| matches!(block.kind, BlockKind::Unattached { .. }))
    );
}

#[test]
fn every_block_and_reporter_has_a_node_with_every_pin_named() {
    let mut script = sample_script(&[]);
    let mut seen = 0;
    visit_nodes(&mut script, &mut |node, body, _| {
        let (title, _, layout) = describe(&body, &[]);
        assert!(!title.is_empty(), "{node:?}");
        for item in &layout.left {
            if let Item::Input { name, .. } | Item::Field { name, .. } = item {
                assert!(!name.is_empty(), "{title}: an unnamed pin");
            }
        }
        let named = match &body {
            Body::Block(kind) => block_names(kind).1.len(),
            Body::Value(value) => value_names(value).1.len(),
        };
        let items = layout
            .left
            .iter()
            .filter(|item| !matches!(item, Item::ExecIn))
            .count();
        assert_eq!(named, items, "{title} names {named} of its {items} pins");
        seen += 1;
    });
    assert!(seen > 40, "{seen}");
}

#[test]
fn the_layout_never_changes_the_lua_and_survives_a_save() {
    let mut script = sample_script(&[]);
    let before = lua(&script);
    script.graph.shown = true;
    script
        .graph
        .nodes
        .insert(script.blocks[0].id.to_string(), [10.0, 20.0]);
    script.graph.loose.push(LooseValue::new(number(4.0)));
    assert_eq!(lua(&script), before);
    let text = serde_json::to_string(&script).unwrap();
    let read: BlockScript = serde_json::from_str(&text).unwrap();
    assert_eq!(read, script);
}

#[test]
fn nodes_are_laid_out_without_overlapping() {
    let objects = [ObjectInfo {
        path: "Pad".to_owned(),
        class: ClassName::Part,
        names: Vec::new(),
    }];
    let mut script = sample_script(&objects);
    let ctx = theme::test_context();
    let mut boxes = Vec::new();
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let context = BlockContext {
                key: "layout test",
                client: true,
                highlighted: None,
                objects: &objects,
            };
            let geometry = measure_all(ui, &mut script, &context, &[]);
            let sizes = geometry
                .iter()
                .map(|(node, geometry)| (node.clone(), geometry.size))
                .collect();
            let places = layout(&script, &sizes);
            assert_eq!(places.len(), geometry.len(), "every node is placed");
            for (node, at) in places {
                boxes.push((node.clone(), Rect::from_min_size(at, geometry[&node].size)));
            }
        });
    });
    for (index, (node, a)) in boxes.iter().enumerate() {
        for (other, b) in &boxes[index + 1..] {
            assert!(
                !a.shrink(1.0).intersects(b.shrink(1.0)),
                "{node:?} {a:?} overlaps {other:?} {b:?}"
            );
        }
    }
}

fn run_graph(script: &mut BlockScript, frames: Vec<Vec<egui::Event>>, name: &str) {
    let size = vec2(1500.0, 950.0);
    let editor = |script: &mut BlockScript, ctx: &egui::Context| {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                show_graph(
                    ui,
                    script,
                    BlockContext {
                        key: "graph drag test",
                        client: true,
                        highlighted: None,
                        objects: &[],
                    },
                );
            });
    };
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let mut replay = script.clone();
        let held = frames[..frames.len() - 2].to_vec();
        let path = std::path::PathBuf::from(folder).join(format!("{name}.png"));
        crate::snapshot::snapshot(&path, size, held, |ctx| editor(&mut replay, ctx));
    }
    let ctx = theme::test_context();
    for events in frames {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| editor(script, ctx));
    }
}

/// Where `pin` is on screen in a graph view filling a 1500 by 950 window.
fn pin_on_screen(script: &mut BlockScript, pin: &Pin) -> Pos2 {
    let ctx = theme::test_context();
    let mut found = None;
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let context = BlockContext {
                key: "pin test",
                client: true,
                highlighted: None,
                objects: &[],
            };
            let geometry = measure_all(ui, script, &context, &[]);
            let sizes = geometry
                .iter()
                .map(|(node, geometry)| (node.clone(), geometry.size))
                .collect();
            let places = layout(script, &sizes);
            for (node, geometry) in &geometry {
                for (candidate, offset) in &geometry.pins {
                    if candidate == pin {
                        found = Some(places[node] + *offset);
                    }
                }
            }
        });
    });
    let origin = pos2(0.0, TOOLBAR_HEIGHT) + GraphState::default().pan;
    origin + found.expect("the pin is drawn").to_vec2()
}

#[test]
fn dragging_a_wire_from_an_event_to_a_node_wires_them() {
    let hello = print("hello");
    let hello_id = hello.id;
    let start = Block::new(BlockKind::WhenStarted { body: Vec::new() });
    let start_id = start.id;
    let mut script = BlockScript {
        blocks: vec![start, loose(vec![hello])],
        graph: Graph {
            shown: true,
            nodes: BTreeMap::from([
                (start_id.to_string(), [40.0, 40.0]),
                (hello_id.to_string(), [420.0, 220.0]),
            ]),
            ..Default::default()
        },
    };
    let from = pin_on_screen(&mut script, &Pin::ExecOut(start_id, Out::Body(0)));
    let to = pin_on_screen(&mut script, &Pin::ExecIn(hello_id));
    run_graph(&mut script, drag_frames(from, to), "graph-wire");
    assert_eq!(
        lua(&script),
        "task.spawn(function()\n    print(\"hello\")\nend)\n",
        "{script:#?}"
    );
}

#[test]
fn the_graph_draws_in_both_themes_without_changing_the_script() {
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
    let mut script = sample_script(&objects);
    // A small script, as Code switched to Graph would be, with a loose
    // value node.
    script.blocks.truncate(2);
    if let BlockKind::WhenStarted { body } = &mut script.blocks[0].kind {
        body.truncate(6);
    }
    script.graph.shown = true;
    script.graph.loose.push(LooseValue::new(Expression::Join {
        left: Box::new(Expression::Text {
            value: "Hi ".to_owned(),
        }),
        right: Box::new(Expression::Variable {
            name: "score".to_owned(),
        }),
    }));
    let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(std::path::PathBuf::from);
    for (theme, name) in [(egui::Theme::Dark, "dark"), (egui::Theme::Light, "light")] {
        let mut drawn = script.clone();
        let mut show_all = |ctx: &egui::Context| {
            ctx.set_theme(theme);
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    show_graph(
                        ui,
                        &mut drawn,
                        BlockContext {
                            key: "graph test",
                            client: true,
                            highlighted: None,
                            objects: &objects,
                        },
                    );
                });
        };
        match &snapshots {
            Some(folder) => crate::snapshot::snapshot(
                &folder.join(format!("graph-{name}.png")),
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

/// Frames that type `text` into the focused box and press Enter.
fn type_and_enter(text: &str) -> Vec<Vec<egui::Event>> {
    let enter = |pressed| egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Default::default(),
    };
    vec![
        Vec::new(),
        vec![egui::Event::Text(text.to_owned())],
        vec![enter(true)],
        vec![enter(false)],
        Vec::new(),
    ]
}

#[test]
fn the_node_menu_finds_a_node_and_adds_it_where_it_opened() {
    let mut script = BlockScript::default();
    let at = pos2(600.0, 400.0);
    let right = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: Default::default(),
    };
    let mut frames = vec![
        vec![egui::Event::PointerMoved(at)],
        vec![right(true)],
        vec![right(false)],
    ];
    frames.extend(type_and_enter("game starts"));
    run_graph(&mut script, frames, "graph-menu");
    assert_eq!(script.blocks.len(), 1, "{script:#?}");
    assert!(matches!(
        script.blocks[0].kind,
        BlockKind::WhenStarted { .. }
    ));
    let place = script.graph.nodes[&script.blocks[0].id.to_string()];
    let expected = at - pos2(0.0, TOOLBAR_HEIGHT).to_vec2() - GraphState::default().pan;
    assert_eq!(place, [expected.x, expected.y]);
}

#[test]
fn a_wire_let_go_on_the_canvas_makes_a_node_wired_to_it() {
    let start = Block::new(BlockKind::WhenStarted { body: Vec::new() });
    let start_id = start.id;
    let mut script = BlockScript {
        blocks: vec![start],
        graph: Graph {
            shown: true,
            nodes: BTreeMap::from([(start_id.to_string(), [40.0, 40.0])]),
            ..Default::default()
        },
    };
    let from = pin_on_screen(&mut script, &Pin::ExecOut(start_id, Out::Body(0)));
    let mut frames = drag_frames(from, from + vec2(300.0, 60.0));
    frames.extend(type_and_enter("wait"));
    run_graph(&mut script, frames, "graph-wire-menu");
    assert_eq!(
        lua(&script),
        "task.spawn(function()\n    task.wait(1)\nend)\n",
        "{script:#?}"
    );
}
