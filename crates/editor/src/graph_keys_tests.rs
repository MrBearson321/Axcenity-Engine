//! Unified Phase 91: the same keys do the same thing in all five graph
//! views, which share `node_graph::canvas`: a block script's Graph view,
//! the Animation tab's state machines, and the node graphs (UI Event
//! Graph, behavior trees, material graphs). One test drives each view with
//! the same key presses.

use axcenity_anim::{StateMachineFile, files::StateDef};
use axcenity_blocks::behavior_tree::BehaviorNodes;
use axcenity_blocks::node_graph::{NodeGraph, NodeLibrary};
use axcenity_blocks::ui_graph::UiNodes;
use axcenity_blocks::{Block, BlockKind, BlockScript, Expression};
use eframe::egui::{self, Event, Key, Modifiers, Pos2, Rect, Vec2, pos2, vec2};

use crate::animation::{ControllerDoc, Selected};
use crate::block_editor::{BlockContext, show_graph, view_of};
use crate::material_graph::MaterialLibrary;
use crate::node_graph::{GraphContext, GraphView};
use crate::theme;

const SIZE: Vec2 = Vec2::new(1400.0, 900.0);

/// One graph view, driven a frame at a time.
trait View {
    fn name(&self) -> &'static str;
    /// Draws a frame with `events`, and returns what it put on the
    /// clipboard.
    fn frame(&mut self, ctx: &egui::Context, events: Vec<Event>) -> Option<String>;
    fn nodes(&self) -> usize;
    /// Selects the first node.
    fn select_first(&mut self, ctx: &egui::Context);
    fn look(&self, ctx: &egui::Context) -> (Vec2, f32);
}

fn run(ctx: &egui::Context, events: Vec<Event>, ui: impl FnMut(&mut egui::Ui)) -> Option<String> {
    let modifiers = events
        .iter()
        .find_map(|event| match event {
            Event::Key { modifiers, .. } => Some(*modifiers),
            _ => None,
        })
        .unwrap_or_default();
    let mut all = vec![
        Event::ModifiersChanged(modifiers),
        Event::PointerMoved(middle()),
    ];
    all.extend(events);
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
        events: all,
        ..Default::default()
    };
    let output = theme::run(ctx, input, ui);
    output
        .platform_output
        .commands
        .into_iter()
        .find_map(|command| match command {
            egui::OutputCommand::CopyText(text) => Some(text),
            _ => None,
        })
}

/// Over the graph, away from its nodes.
fn middle() -> Pos2 {
    pos2(SIZE.x * 0.6, SIZE.y * 0.6)
}

fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

struct Blocks {
    script: BlockScript,
}

impl View for Blocks {
    fn name(&self) -> &'static str {
        "block script Graph view"
    }

    fn frame(&mut self, ctx: &egui::Context, events: Vec<Event>) -> Option<String> {
        run(ctx, events, |ui| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| {
                    show_graph(
                        ui,
                        &mut self.script,
                        BlockContext {
                            key: "graph keys",
                            client: false,
                            highlighted: None,
                            objects: &[],
                        },
                    );
                });
        })
    }

    fn nodes(&self) -> usize {
        self.script
            .blocks
            .iter()
            .map(|block| match &block.kind {
                BlockKind::Unattached { body } => body.len(),
                _ => 1,
            })
            .sum::<usize>()
            + self.script.graph.loose.len()
    }

    fn select_first(&mut self, ctx: &egui::Context) {
        self.frame(ctx, vec![key(Key::A, Modifiers::COMMAND)]);
    }

    fn look(&self, ctx: &egui::Context) -> (Vec2, f32) {
        view_of(ctx, "graph keys")
    }
}

struct Machine {
    doc: ControllerDoc,
}

impl View for Machine {
    fn name(&self) -> &'static str {
        "state machine"
    }

    fn frame(&mut self, ctx: &egui::Context, events: Vec<Event>) -> Option<String> {
        run(ctx, events, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                self.doc.graph_ui(ui, &[]);
            });
        })
    }

    fn nodes(&self) -> usize {
        self.doc.file.states.len()
    }

    fn select_first(&mut self, _: &egui::Context) {
        // A state machine selects one thing at a time: no Ctrl+A.
        self.doc.selected = Selected::Node(crate::animation::Node::State(0));
    }

    fn look(&self, _: &egui::Context) -> (Vec2, f32) {
        (self.doc.view.pan, self.doc.view.zoom)
    }
}

struct Nodes {
    name: &'static str,
    library: &'static dyn NodeLibrary,
    graph: NodeGraph,
    view: GraphView,
}

impl Nodes {
    fn new(name: &'static str, library: &'static dyn NodeLibrary) -> Self {
        let mut graph = NodeGraph::default();
        graph.add_node(&library.specs()[0], [0.0, 0.0]);
        Self {
            name,
            library,
            graph,
            view: GraphView::default(),
        }
    }
}

impl View for Nodes {
    fn name(&self) -> &'static str {
        self.name
    }

    fn frame(&mut self, ctx: &egui::Context, events: Vec<Event>) -> Option<String> {
        let errors = Default::default();
        run(ctx, events, |ui| {
            let rect = Rect::from_min_size(Pos2::ZERO, SIZE);
            self.view.show(
                ui,
                rect,
                &mut self.graph,
                &GraphContext {
                    library: self.library,
                    objects: &[],
                    images: &[],
                    errors: &errors,
                    editable: true,
                    key: self.name,
                },
            );
        })
    }

    fn nodes(&self) -> usize {
        self.graph.nodes.len()
    }

    fn select_first(&mut self, ctx: &egui::Context) {
        self.frame(ctx, vec![key(Key::A, Modifiers::COMMAND)]);
    }

    fn look(&self, _: &egui::Context) -> (Vec2, f32) {
        (self.view.pan, self.view.zoom)
    }
}

fn views() -> Vec<Box<dyn View>> {
    let print = Block::new(BlockKind::Print {
        value: Expression::Text {
            value: "hi".to_owned(),
        },
    });
    let blocks = Blocks {
        script: BlockScript {
            blocks: vec![Block::new(BlockKind::Unattached { body: vec![print] })],
            ..Default::default()
        },
    };
    let file = StateMachineFile {
        default_state: "Idle".into(),
        states: vec![StateDef::new(
            "Idle",
            axcenity_anim::files::Motion::Clip(String::new()),
            [0.0, 0.0],
        )],
        ..StateMachineFile::default()
    };
    let machine = Machine {
        doc: ControllerDoc::open(
            "assets/animations/keys.statemachine.json".into(),
            file,
            None,
        ),
    };
    vec![
        Box::new(blocks),
        Box::new(machine),
        Box::new(Nodes::new("UI Event Graph", &UiNodes)),
        Box::new(Nodes::new("behavior tree", &BehaviorNodes)),
        Box::new(Nodes::new("material graph", &MaterialLibrary)),
    ]
}

#[test]
fn the_same_keys_work_in_all_five_graph_views() {
    for mut view in views() {
        let ctx = theme::test_context();
        let name = view.name();
        view.frame(&ctx, Vec::new());
        view.frame(&ctx, Vec::new());
        assert_eq!(view.nodes(), 1, "{name}");

        // Ctrl+D duplicates the selection, and Delete deletes the copy,
        // which is what's selected then.
        view.select_first(&ctx);
        view.frame(&ctx, vec![key(Key::D, Modifiers::COMMAND)]);
        view.frame(&ctx, Vec::new());
        assert_eq!(view.nodes(), 2, "{name}: Ctrl+D");
        view.frame(&ctx, vec![key(Key::Delete, Modifiers::NONE)]);
        view.frame(&ctx, Vec::new());
        assert_eq!(view.nodes(), 1, "{name}: Delete");

        // Ctrl+C copies the selection as text, and Ctrl+V pastes it.
        view.select_first(&ctx);
        let copied = view
            .frame(&ctx, vec![Event::Copy])
            .unwrap_or_else(|| panic!("{name}: Ctrl+C copied nothing"));
        view.frame(&ctx, vec![Event::Paste(copied)]);
        view.frame(&ctx, Vec::new());
        assert_eq!(view.nodes(), 2, "{name}: Ctrl+V");

        // The wheel pans and F frames the selection again.
        let wheel = Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(-300.0, -300.0),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        };
        let before = view.look(&ctx);
        view.frame(&ctx, vec![wheel]);
        view.frame(&ctx, Vec::new());
        let panned = view.look(&ctx);
        assert_ne!(panned, before, "{name}: the wheel pans");
        view.select_first(&ctx);
        view.frame(&ctx, vec![key(Key::F, Modifiers::NONE)]);
        view.frame(&ctx, Vec::new());
        view.frame(&ctx, Vec::new());
        assert_ne!(view.look(&ctx), panned, "{name}: F frames");

        // Space opens the search menu, its box ready to type in, and
        // Escape closes it.
        view.frame(&ctx, vec![key(Key::Space, Modifiers::NONE)]);
        view.frame(&ctx, Vec::new());
        assert!(
            ctx.memory(|memory| memory.focused().is_some()),
            "{name}: Space opens the search"
        );
        view.frame(&ctx, vec![key(Key::Escape, Modifiers::NONE)]);
        view.frame(&ctx, Vec::new());
        view.frame(&ctx, vec![key(Key::D, Modifiers::COMMAND)]);
        assert!(
            !ctx.memory(|memory| memory.focused().is_some()),
            "{name}: Escape closes it"
        );
    }
}
