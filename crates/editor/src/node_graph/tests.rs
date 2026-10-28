use axcenity_blocks::ui_graph::{UI_NODES, UiNodes};

use super::*;

const SIZE: Vec2 = vec2(1200.0, 800.0);

fn spec(kind: &str) -> &'static NodeSpec {
    UI_NODES.iter().find(|spec| spec.kind == kind).unwrap()
}

/// A Button Activated at the left and a Show Page to its right, not wired.
fn two_nodes() -> NodeGraph {
    let mut graph = NodeGraph::default();
    graph.add_node(spec("button_activated"), [0.0, 0.0]);
    graph.add_node(spec("show_page"), [400.0, 0.0]);
    graph
}

/// The editor in a window, with a view kept between frames.
struct Harness {
    ctx: egui::Context,
    view: GraphView,
    graph: NodeGraph,
    /// Every finished edit's "before", in order.
    steps: Vec<NodeGraph>,
    /// What was copied to the clipboard.
    copied: Vec<String>,
    frames: usize,
}

impl Harness {
    fn new(graph: NodeGraph) -> Self {
        let mut harness = Self {
            ctx: theme::test_context(),
            view: GraphView::default(),
            graph,
            steps: Vec::new(),
            copied: Vec::new(),
            frames: 0,
        };
        // The first frame fits the nodes into view; then pins are known.
        harness.run(Vec::new());
        harness.run(Vec::new());
        harness
    }

    fn run(&mut self, events: Vec<egui::Event>) {
        self.run_with(events, egui::Modifiers::NONE);
    }

    fn run_with(&mut self, events: Vec<egui::Event>, modifiers: egui::Modifiers) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            events,
            modifiers,
            time: Some(self.frames as f64 / 30.0),
            ..Default::default()
        };
        self.frames += 1;
        let errors = BTreeMap::new();
        let output = self.ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let rect = ui.max_rect();
                let response = self.view.show(
                    ui,
                    rect,
                    &mut self.graph,
                    &GraphContext {
                        library: &UiNodes,
                        objects: &[],
                        images: &[],
                        errors: &errors,
                        editable: true,
                        key: "test",
                    },
                );
                self.steps.extend(response.step);
            });
        });
        for command in output.platform_output.commands {
            if let egui::OutputCommand::CopyText(text) = command {
                self.copied.push(text);
            }
        }
    }

    fn drag(&mut self, from: Pos2, to: Pos2, button: egui::PointerButton) {
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: Default::default(),
        };
        self.run(vec![egui::Event::PointerMoved(from)]);
        self.run(vec![press(from, true)]);
        for step in 1..=4 {
            self.run(vec![egui::Event::PointerMoved(
                from.lerp(to, step as f32 / 4.0),
            )]);
        }
        self.run(vec![press(to, false)]);
        self.run(Vec::new());
    }

    fn key(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
        let event = egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        self.run_with(vec![event], modifiers);
    }

    fn pin(&self, node: u32, pin: &str, output: bool) -> Pos2 {
        self.view
            .pin_on_screen(&PinRef::new(node, pin), output)
            .expect("the pin is on screen")
    }

    /// A point on the canvas with no node near it.
    fn empty_spot(&self) -> Pos2 {
        let canvas = self.view.canvas.shrink(30.0);
        (0..20)
            .flat_map(|x| (0..20).map(move |y| (x, y)))
            .map(|(x, y)| {
                pos2(
                    canvas.left() + canvas.width() * x as f32 / 19.0,
                    canvas.top() + canvas.height() * y as f32 / 19.0,
                )
            })
            .find(|at| {
                self.view
                    .rects
                    .iter()
                    .all(|(_, rect)| !rect.expand(40.0).contains(*at))
            })
            .expect("somewhere is empty")
    }
}

#[test]
fn dragging_between_pins_wires_them_from_either_end_in_one_step() {
    let mut harness = Harness::new(two_nodes());
    let (out, input) = (harness.pin(1, "", true), harness.pin(2, "", false));
    harness.drag(out, input, egui::PointerButton::Primary);
    assert_eq!(
        harness.graph.wires,
        vec![axcenity_blocks::node_graph::Wire {
            from: PinRef::new(1, ""),
            to: PinRef::new(2, ""),
        }]
    );
    assert_eq!(
        harness.steps,
        [two_nodes()],
        "one step, from before the wire"
    );

    // From the input end, back to the output, makes the same wire again.
    harness.graph.wires.clear();
    harness.drag(input, out, egui::PointerButton::Primary);
    assert_eq!(harness.graph.wires.len(), 1);
}

#[test]
fn a_wire_that_doesnt_fit_is_refused_with_a_notice() {
    let mut graph = two_nodes();
    graph.add_node(spec("get_text"), [0.0, 200.0]);
    let mut harness = Harness::new(graph);
    // Text into the exec input: refused.
    let (text, exec) = (harness.pin(3, "Text", true), harness.pin(2, "", false));
    harness.drag(text, exec, egui::PointerButton::Primary);
    assert!(harness.graph.wires.is_empty());
    // Text into Page: fits.
    let page = harness.pin(2, "Page", false);
    harness.drag(text, page, egui::PointerButton::Primary);
    assert_eq!(harness.graph.wires.len(), 1);
}

#[test]
fn a_wire_dropped_on_the_canvas_opens_the_menu_and_wires_the_new_node() {
    let mut graph = NodeGraph::default();
    graph.add_node(spec("button_activated"), [0.0, 0.0]);
    let mut harness = Harness::new(graph);
    let out = harness.pin(1, "", true);
    let spot = harness.empty_spot();
    harness.drag(out, spot, egui::PointerButton::Primary);
    assert!(harness.view.menu.is_some(), "the menu opened");
    // Only nodes with an exec input fit an exec wire.
    let fits: Vec<&str> = menu::fitting(
        &harness.graph,
        &UiNodes,
        harness.view.menu.as_ref().unwrap().from.as_ref(),
    )
    .iter()
    .map(|spec| spec.kind)
    .collect();
    assert!(
        fits.contains(&"show_page") && !fits.contains(&"get_text"),
        "{fits:?}"
    );

    harness.run(vec![egui::Event::Text("show page".to_owned())]);
    harness.key(egui::Key::Enter, egui::Modifiers::NONE);
    harness.run(Vec::new());
    let made = harness
        .graph
        .nodes
        .iter()
        .find(|node| node.kind == "show_page")
        .expect("a Show Page was made");
    assert_eq!(
        harness.graph.wires,
        vec![axcenity_blocks::node_graph::Wire {
            from: PinRef::new(1, ""),
            to: PinRef::new(made.id, ""),
        }]
    );
    assert!(harness.view.selected.contains(&made.id));
}

#[test]
fn a_box_selects_nodes_and_delete_removes_them_as_one_step() {
    let mut graph = two_nodes();
    graph.add_node(spec("print"), [0.0, 600.0]);
    let mut harness = Harness::new(graph);
    let first = harness.view.node_on_screen(1).unwrap();
    let second = harness.view.node_on_screen(2).unwrap();
    // A box around the top two, from empty canvas above and left of them.
    harness.drag(
        first.min - vec2(20.0, 20.0),
        pos2(second.right() + 10.0, second.bottom() + 10.0),
        egui::PointerButton::Primary,
    );
    assert_eq!(harness.view.selected, BTreeSet::from([1, 2]));
    assert!(harness.steps.is_empty(), "selecting isn't an edit");

    let at = harness.empty_spot();
    harness.run(vec![egui::Event::PointerMoved(at)]);
    harness.key(egui::Key::Delete, egui::Modifiers::NONE);
    let kinds: Vec<&str> = harness
        .graph
        .nodes
        .iter()
        .map(|node| node.kind.as_str())
        .collect();
    assert_eq!(kinds, ["print"]);
    assert_eq!(harness.steps.len(), 1);
}

#[test]
fn copy_and_paste_go_through_the_clipboard_as_text() {
    let mut graph = two_nodes();
    graph
        .connect(&UiNodes, PinRef::new(1, ""), PinRef::new(2, ""))
        .unwrap();
    let mut harness = Harness::new(graph);
    harness.view.selected = BTreeSet::from([1, 2]);
    let at = harness.empty_spot();
    harness.run(vec![egui::Event::PointerMoved(at)]);
    harness.run(vec![egui::Event::Copy]);
    let copied = harness
        .copied
        .last()
        .expect("copied to the clipboard")
        .clone();
    assert!(copied.contains("button_activated"), "{copied}");

    harness.run(vec![egui::Event::Paste(copied)]);
    assert_eq!(harness.graph.nodes.len(), 4);
    assert_eq!(
        harness.graph.wires.len(),
        2,
        "the copies are wired as the originals"
    );
    assert_eq!(
        harness.view.selected.len(),
        2,
        "the pasted nodes are selected"
    );
    assert!(!harness.view.selected.contains(&1));

    // Ctrl+D duplicates the selection again.
    harness.key(egui::Key::D, egui::Modifiers::COMMAND);
    assert_eq!(harness.graph.nodes.len(), 6);
    assert_eq!(
        harness.steps.len(),
        2,
        "paste and duplicate are a step each"
    );
}

#[test]
fn dragging_a_node_moves_the_selection_in_one_step() {
    let mut harness = Harness::new(two_nodes());
    harness.view.selected = BTreeSet::from([1, 2]);
    let header = harness.view.node_on_screen(1).unwrap().left_top() + vec2(30.0, 8.0);
    let zoom = harness.view.zoom;
    harness.drag(
        header,
        header + vec2(100.0, 50.0),
        egui::PointerButton::Primary,
    );
    let moved = |id: u32| harness.graph.node(id).unwrap().position;
    let expect = |x: f32, y: f32| {
        assert!(
            (x - 100.0 / zoom).abs() < 1.0 && (y - 50.0 / zoom).abs() < 1.0,
            "{x}, {y}"
        );
    };
    expect(moved(1)[0], moved(1)[1]);
    expect(moved(2)[0] - 400.0, moved(2)[1]);
    assert_eq!(harness.steps.len(), 1, "the whole drag is one step");
}

#[test]
fn a_comment_goes_around_the_selection_and_moves_its_nodes() {
    let mut harness = Harness::new(two_nodes());
    harness.view.selected = BTreeSet::from([1, 2]);
    harness.view.comment_selection(&mut harness.graph);
    let comment = harness.graph.comments[0].clone();
    assert!(
        comment.position[0] < 0.0 && comment.position[1] < 0.0,
        "{comment:?}"
    );
    assert!(comment.size[0] > 400.0, "{comment:?}");
    harness.view.renaming = None;
    harness.run(Vec::new());
    harness.run(Vec::new());

    // Dragging its title moves it and both nodes inside.
    let z = harness.view.zoom;
    let origin = harness.view.node_on_screen(1).unwrap().min
        - vec2(-comment.position[0], -comment.position[1]) * z;
    let title = origin + vec2(60.0, COMMENT_HEADER * 0.5 * z);
    harness.drag(title, title + vec2(0.0, 80.0), egui::PointerButton::Primary);
    let dy = 80.0 / z;
    assert!((harness.graph.comments[0].position[1] - comment.position[1] - dy).abs() < 1.0);
    assert!((harness.graph.node(1).unwrap().position[1] - dy).abs() < 1.0);
    assert!((harness.graph.node(2).unwrap().position[1] - dy).abs() < 1.0);
}

#[test]
fn a_node_with_errors_is_taller_and_lists_them() {
    let ctx = theme::test_context();
    // Fonts are there from the first frame.
    let _ = ctx.run(Default::default(), |_| {});
    let graph = two_nodes();
    let node = graph.node(2).unwrap();
    let plain = node_shape(&ctx, spec("show_page"), node, &graph, &[]);
    let errors = vec!["Pick a PageLayout for Pages.".to_owned()];
    let wrong = node_shape(&ctx, spec("show_page"), node, &graph, &errors);
    assert!(wrong.size.y > plain.size.y);
    assert_eq!(wrong.errors, errors);
    // The pins don't move for it.
    assert_eq!(wrong.pin_offset("", false), plain.pin_offset("", false));
}

#[test]
fn the_editor_draws_with_errors_comments_and_every_node_kind() {
    let mut graph = NodeGraph::default();
    for (index, spec) in UI_NODES.iter().enumerate() {
        let x = (index % 5) as f32 * 260.0;
        let y = (index / 5) as f32 * 220.0;
        graph.add_node(spec, [x, y]);
    }
    graph
        .connect(&UiNodes, PinRef::new(1, ""), PinRef::new(6, ""))
        .unwrap();
    graph.add_comment([-20.0, -50.0], [520.0, 240.0]);
    let errors = BTreeMap::from([(
        6,
        vec!["Value isn't connected: wire a value in, or type one.".to_owned()],
    )]);
    let mut view = GraphView::default();
    let draw = |ctx: &egui::Context, view: &mut GraphView, graph: &mut NodeGraph| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let rect = ui.max_rect();
            view.show(
                ui,
                rect,
                graph,
                &GraphContext {
                    library: &UiNodes,
                    objects: &[],
                    images: &[],
                    errors: &errors,
                    editable: true,
                    key: "draw test",
                },
            );
        });
    };
    let ctx = theme::test_context();
    for _ in 0..2 {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| draw(ctx, &mut view, &mut graph));
    }
    assert_eq!(view.rects.len(), UI_NODES.len());
    let before = graph.clone();
    let _ = ctx.run(Default::default(), |ctx| draw(ctx, &mut view, &mut graph));
    assert_eq!(graph, before, "drawing never changes the graph");
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS").map(std::path::PathBuf::from) {
        let mut view = GraphView::default();
        let mut graph = graph.clone();
        crate::snapshot::snapshot(
            &folder.join("node-graph.png"),
            SIZE,
            vec![Vec::new(); 3],
            |ctx| draw(ctx, &mut view, &mut graph),
        );
    }
}
