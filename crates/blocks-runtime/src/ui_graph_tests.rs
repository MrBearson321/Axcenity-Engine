use std::collections::BTreeSet;

use crate::node_graph::*;
use crate::ui_graph::*;

fn spec(kind: &str) -> &'static NodeSpec {
    UI_NODES.iter().find(|spec| spec.kind == kind).unwrap()
}

fn add(graph: &mut NodeGraph, kind: &str, values: &[(&str, &str)]) -> u32 {
    let id = graph.add_node(spec(kind), [0.0, 0.0]);
    let node = graph.node_mut(id).unwrap();
    for (name, value) in values {
        node.values.insert((*name).to_owned(), (*value).to_owned());
    }
    id
}

fn wire(graph: &mut NodeGraph, from: (u32, &str), to: (u32, &str)) {
    graph
        .connect(
            &UiNodes,
            PinRef::new(from.0, from.1),
            PinRef::new(to.0, to.1),
        )
        .unwrap();
}

fn gui(path: &str, class: &str) -> GraphObject {
    GraphObject {
        root: ObjectRoot::Gui,
        path: path.to_owned(),
        class: class.to_owned(),
    }
}

/// A menu: a ScreenGui with a Play button and a PageLayout of two pages.
fn menu_objects() -> Vec<GraphObject> {
    vec![
        gui("", "ScreenGui"),
        gui("Play", "TextButton"),
        gui("Pages", "PageLayout"),
        gui("Pages/Home", "Frame"),
        gui("Pages/Settings", "Frame"),
        gui("Name Box", "TextBox"),
        gui("Score", "TextLabel"),
    ]
}

#[test]
fn button_activated_to_show_page_compiles_to_ordinary_lua() {
    let mut graph = NodeGraph::default();
    let clicked = add(&mut graph, "button_activated", &[("Button", "Play")]);
    let show = add(
        &mut graph,
        "show_page",
        &[("Pages", "Pages"), ("Page", "Settings")],
    );
    wire(&mut graph, (clicked, ""), (show, ""));

    let compiled = compile_ui_graph(&graph, &menu_objects()).unwrap();
    assert_eq!(
        compiled.source,
        format!(
            "{GENERATED_HEADER}\nlocal gui = script.Parent\n\n\
             gui.Play.Activated:Connect(function()\n    \
             gui.Pages.CurrentPage = \"Settings\"\nend)\n\n"
        )
    );
    // Line 4 is the event, 5 the Show Page: a runtime error there points at it.
    assert_eq!(compiled.node_at(4), Some(clicked));
    assert_eq!(compiled.node_at(5), Some(show));
}

#[test]
fn parallel_starts_each_wired_branch_at_once_in_its_own_task() {
    let mut graph = NodeGraph::default();
    let clicked = add(&mut graph, "button_activated", &[("Button", "Play")]);
    let parallel = add(&mut graph, "parallel", &[]);
    let wait = add(&mut graph, "delay", &[("Seconds", "2")]);
    let hide = add(
        &mut graph,
        "set_visible",
        &[("Element", "Score"), ("Visible", "false")],
    );
    let show = add(
        &mut graph,
        "show_page",
        &[("Pages", "Pages"), ("Page", "Settings")],
    );
    wire(&mut graph, (clicked, ""), (parallel, ""));
    wire(&mut graph, (parallel, "Branch 0"), (wait, ""));
    wire(&mut graph, (wait, "Completed"), (hide, ""));
    // Branch 1 is left empty; Branch 2 still runs.
    wire(&mut graph, (parallel, "Branch 2"), (show, ""));

    let compiled = compile_ui_graph(&graph, &menu_objects()).unwrap();
    assert_eq!(
        compiled.source,
        format!(
            "{GENERATED_HEADER}\nlocal gui = script.Parent\n\n\
             gui.Play.Activated:Connect(function()\n    \
             task.spawn(function()\n        \
             task.wait(2)\n        \
             gui.Score.Visible = false\n    \
             end)\n    \
             task.spawn(function()\n        \
             gui.Pages.CurrentPage = \"Settings\"\n    \
             end)\nend)\n\n"
        )
    );
}

#[test]
fn an_unconnected_required_pin_is_an_error_on_its_node_and_no_lua() {
    let mut graph = NodeGraph::default();
    let clicked = add(&mut graph, "button_activated", &[("Button", "Play")]);
    let set = add(
        &mut graph,
        "set_property",
        &[("Object", "Score"), ("Property", "Text")],
    );
    wire(&mut graph, (clicked, ""), (set, ""));

    let errors = compile_ui_graph(&graph, &menu_objects()).unwrap_err();
    assert_eq!(
        errors,
        vec![GraphError {
            node: Some(set),
            message: "Value isn't connected: wire a value in, or type one.".to_owned(),
        }]
    );
    assert_eq!(
        errors_by_node(&errors).keys().copied().collect::<Vec<_>>(),
        [set]
    );
}

#[test]
fn objects_that_arent_there_or_are_the_wrong_class_are_errors_on_their_nodes() {
    let mut graph = NodeGraph::default();
    let unpicked = add(&mut graph, "button_activated", &[]);
    let gone = add(&mut graph, "button_activated", &[("Button", "Quit")]);
    let label = add(&mut graph, "button_activated", &[("Button", "Score")]);
    for event in [unpicked, gone, label] {
        let print = add(&mut graph, "print", &[]);
        wire(&mut graph, (event, ""), (print, ""));
    }
    let errors = compile_ui_graph(&graph, &menu_objects()).unwrap_err();
    let messages: Vec<(Option<u32>, &str)> = errors
        .iter()
        .map(|error| (error.node, error.message.as_str()))
        .collect();
    assert_eq!(
        messages,
        [
            (Some(unpicked), "Pick a TextButton for Button."),
            (
                Some(gone),
                "There's no `Quit` in this ScreenGui now. Pick another TextButton."
            ),
            (Some(label), "`Score` is a TextLabel, not a TextButton."),
        ]
    );
}

#[test]
fn nodes_not_wired_to_an_event_dont_run_or_complain() {
    let mut graph = NodeGraph::default();
    add(&mut graph, "set_property", &[]);
    add(&mut graph, "button_activated", &[]);
    let compiled = compile_ui_graph(&graph, &menu_objects()).unwrap();
    assert_eq!(
        compiled.source,
        format!("{GENERATED_HEADER}\nlocal gui = script.Parent\n")
    );
}

#[test]
fn data_nodes_flow_and_variables_make_expressions() {
    let mut graph = NodeGraph::default();
    graph.variables.push(Variable {
        name: "clicks".to_owned(),
        value: "0".to_owned(),
    });
    let clicked = add(&mut graph, "button_activated", &[("Button", "Play")]);
    let get = add(&mut graph, "get_variable", &[("Variable", "clicks")]);
    let plus = add(&mut graph, "math", &[("Operator", "+"), ("B", "1")]);
    wire(&mut graph, (get, "Value"), (plus, "A"));
    let set = add(&mut graph, "set_variable", &[("Variable", "clicks")]);
    wire(&mut graph, (plus, "Result"), (set, "Value"));
    wire(&mut graph, (clicked, ""), (set, ""));
    let branch = add(&mut graph, "branch", &[]);
    let compare = add(&mut graph, "compare", &[("Operator", "≥"), ("B", "3")]);
    wire(&mut graph, (get, "Value"), (compare, "A"));
    wire(&mut graph, (compare, "Result"), (branch, "Condition"));
    wire(&mut graph, (set, ""), (branch, ""));
    let join = add(&mut graph, "join", &[("A", "Clicks: ")]);
    wire(&mut graph, (get, "Value"), (join, "B"));
    let show = add(
        &mut graph,
        "set_property",
        &[("Object", "Score"), ("Property", "Text")],
    );
    wire(&mut graph, (join, "Result"), (show, "Value"));
    wire(&mut graph, (branch, "True"), (show, ""));
    let wait = add(&mut graph, "delay", &[]);
    wire(&mut graph, (branch, "False"), (wait, ""));
    let hide = add(
        &mut graph,
        "set_visible",
        &[("Element", ""), ("Visible", "false")],
    );
    wire(&mut graph, (wait, "Completed"), (hide, ""));

    let compiled = compile_ui_graph(&graph, &menu_objects()).unwrap();
    assert_eq!(
        compiled.source,
        format!(
            "{GENERATED_HEADER}\n\
             local gui = script.Parent\n\
             local clicks = 0\n\
             \n\
             gui.Play.Activated:Connect(function()\n\
             \x20   clicks = (clicks + 1)\n\
             \x20   if (clicks >= 3) then\n\
             \x20       gui.Score.Text = (\"Clicks: \" .. clicks)\n\
             \x20   else\n\
             \x20       task.wait(1)\n\
             \x20       gui.Enabled = false\n\
             \x20   end\n\
             end)\n\n"
        )
    );
}

#[test]
fn every_frame_screen_shown_focus_lost_tween_sound_and_remote_events() {
    let mut objects = menu_objects();
    objects.push(GraphObject {
        root: ObjectRoot::Workspace,
        path: "Pad/Chime".to_owned(),
        class: "Sound".to_owned(),
    });
    objects.push(GraphObject {
        root: ObjectRoot::ReplicatedStorage,
        path: "Named".to_owned(),
        class: "RemoteEvent".to_owned(),
    });
    let mut graph = NodeGraph::default();
    let frame = add(&mut graph, "every_frame", &[]);
    let print = add(&mut graph, "print", &[]);
    wire(&mut graph, (frame, "Delta Time"), (print, "Value"));
    wire(&mut graph, (frame, ""), (print, ""));
    let shown = add(&mut graph, "screen_shown", &[]);
    let tween = add(&mut graph, "tween", &[("Object", ""), ("Goal", "0")]);
    wire(&mut graph, (shown, ""), (tween, ""));
    let typed = add(&mut graph, "focus_lost", &[("TextBox", "Name Box")]);
    let chime = add(&mut graph, "play_sound", &[("Sound", "Pad/Chime")]);
    let fire = add(&mut graph, "fire_remote", &[("RemoteEvent", "Named")]);
    wire(&mut graph, (typed, ""), (chime, ""));
    wire(&mut graph, (chime, ""), (fire, ""));
    wire(&mut graph, (typed, "Text"), (fire, "Value"));

    let source = compile_ui_graph(&graph, &objects).unwrap().source;
    for expected in [
        "local ReplicatedStorage = game:GetService(\"ReplicatedStorage\")\n\
         local RunService = game:GetService(\"RunService\")\n\
         local TweenService = game:GetService(\"TweenService\")\n",
        "RunService.Heartbeat:Connect(function(deltaTime)\n    print(tostring(deltaTime))\nend)",
        "    local function screenShown()\n        TweenService:Create(gui, TweenInfo.new(0.5, \
         Enum.EasingStyle.Quad, Enum.EasingDirection.Out), {BackgroundTransparency = 0}):Play()\n",
        "    if gui.Enabled then\n        task.spawn(screenShown)\n    end\n",
        "gui[\"Name Box\"].FocusLost:Connect(function()\n    workspace.Pad.Chime:Play()\n    \
         ReplicatedStorage.Named:FireServer(gui[\"Name Box\"].Text)\nend)",
    ] {
        assert!(source.contains(expected), "{expected}\n---\n{source}");
    }
}

#[test]
fn a_value_from_another_event_is_an_error_on_the_node_using_it() {
    let mut graph = NodeGraph::default();
    let frame = add(&mut graph, "every_frame", &[]);
    let clicked = add(&mut graph, "button_activated", &[("Button", "Play")]);
    let print = add(&mut graph, "print", &[]);
    wire(&mut graph, (clicked, ""), (print, ""));
    wire(&mut graph, (frame, "Delta Time"), (print, "Value"));
    let errors = compile_ui_graph(&graph, &menu_objects()).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].node, Some(print));
    assert!(errors[0].message.contains("another event"), "{errors:?}");
}

#[test]
fn wires_are_type_checked_and_never_loop() {
    let mut graph = NodeGraph::default();
    let clicked = add(&mut graph, "button_activated", &[]);
    let first = add(&mut graph, "print", &[]);
    let second = add(&mut graph, "print", &[]);
    let branch = add(&mut graph, "branch", &[]);
    let text = add(&mut graph, "get_text", &[]);
    let plus = add(&mut graph, "math", &[]);
    let more = add(&mut graph, "math", &[]);
    let pin = PinRef::new;

    assert!(
        graph
            .connect(&UiNodes, pin(text, "Text"), pin(branch, "Condition"))
            .is_err()
    );
    assert!(
        graph
            .connect(&UiNodes, pin(clicked, ""), pin(text, "Text"))
            .is_err()
    );
    assert!(
        graph
            .connect(&UiNodes, pin(first, ""), pin(first, ""))
            .is_err()
    );
    // A number fits text.
    graph
        .connect(&UiNodes, pin(plus, "Result"), pin(first, "Value"))
        .unwrap();

    wire(&mut graph, (clicked, ""), (first, ""));
    wire(&mut graph, (first, ""), (second, ""));
    wire(&mut graph, (plus, "Result"), (more, "A"));
    let before = graph.clone();
    assert_eq!(
        graph.connect(&UiNodes, pin(second, ""), pin(first, "")),
        Err("That wire would loop back into its own chain.")
    );
    assert_eq!(
        graph.connect(&UiNodes, pin(more, "Result"), pin(plus, "B")),
        Err("That wire would make a value depend on itself.")
    );
    assert_eq!(graph, before, "a refused wire changes nothing");

    // An exec output takes one wire: the new one replaces it.
    wire(&mut graph, (clicked, ""), (second, ""));
    assert_eq!(graph.wires_from(&pin(clicked, "")).count(), 1);
    // An input takes one wire too.
    wire(&mut graph, (more, "Result"), (first, "Value"));
    assert_eq!(graph.wires_into(&pin(first, "Value")).count(), 1);
}

#[test]
fn copied_nodes_paste_with_new_ids_their_wires_and_variables() {
    let mut graph = NodeGraph::default();
    graph.variables.push(Variable {
        name: "score".to_owned(),
        value: "0".to_owned(),
    });
    let clicked = add(&mut graph, "button_activated", &[("Button", "Play")]);
    let set = add(
        &mut graph,
        "set_variable",
        &[("Variable", "score"), ("Value", "1")],
    );
    let outside = add(&mut graph, "print", &[]);
    wire(&mut graph, (clicked, ""), (set, ""));
    wire(&mut graph, (set, ""), (outside, ""));
    graph.node_mut(set).unwrap().position = [100.0, 40.0];
    graph.node_mut(clicked).unwrap().position = [20.0, 40.0];
    let comment = graph.add_comment([0.0, 0.0], [300.0, 120.0]);

    let copied = graph.copy(&BTreeSet::from([clicked, set]), &BTreeSet::from([comment]));
    assert_eq!(copied.wires.len(), 1, "only the wire between copied nodes");

    let mut other = NodeGraph::default();
    let (nodes, comments) = other.paste(&copied, [500.0, 500.0]);
    assert_eq!(nodes.len(), 2);
    assert_eq!(comments.len(), 1);
    assert_eq!(other.variables, graph.variables);
    let pasted_set = other
        .nodes
        .iter()
        .find(|node| node.kind == "set_variable")
        .unwrap();
    assert_eq!(pasted_set.position, [600.0, 540.0]);
    let pasted_click = other
        .nodes
        .iter()
        .find(|node| node.kind == "button_activated")
        .unwrap();
    assert_eq!(
        other.wires,
        vec![Wire {
            from: PinRef::new(pasted_click.id, ""),
            to: PinRef::new(pasted_set.id, ""),
        }]
    );

    // Pasting into the same graph again gives ids nothing has.
    let (again, _) = graph.paste(&copied, [0.0, 300.0]);
    assert!(again.iter().all(|id| *id > comment));
    assert_eq!(
        graph.variables.len(),
        1,
        "a variable it has isn't added twice"
    );
}

#[test]
fn a_graph_file_round_trips_and_an_empty_one_is_small() {
    let mut graph = NodeGraph::default();
    let clicked = add(&mut graph, "button_activated", &[("Button", "Play")]);
    let show = add(
        &mut graph,
        "show_page",
        &[("Pages", "Pages"), ("Page", "Settings")],
    );
    wire(&mut graph, (clicked, ""), (show, ""));
    graph.add_comment([0.0, 0.0], [10.0, 10.0]);
    let text = serde_json::to_string_pretty(&graph).unwrap();
    assert_eq!(serde_json::from_str::<NodeGraph>(&text).unwrap(), graph);
    assert_eq!(
        serde_json::to_string(&NodeGraph::default()).unwrap(),
        r#"{"format_version":1,"nodes":[],"wires":[]}"#
    );
    assert!(serde_json::from_str::<NodeGraph>(r#"{"nodez":[]}"#).is_err());
}

#[test]
fn every_node_kind_is_unique_and_its_pins_and_fields_are_named_once() {
    let mut kinds = BTreeSet::new();
    for spec in UI_NODES {
        assert!(kinds.insert(spec.kind), "{} twice", spec.kind);
        let mut names = BTreeSet::new();
        for field in spec.fields {
            assert!(names.insert(field.name), "{}: {}", spec.kind, field.name);
        }
        for pin in spec.inputs.iter().filter(|pin| pin.ty != PinType::Exec) {
            assert!(names.insert(pin.name), "{}: {}", spec.kind, pin.name);
        }
    }
}

#[test]
fn graph_paths_name_their_screen_gui() {
    assert_eq!(ui_graph_path("Menu"), "ui/Menu.uigraph.json");
    assert!(is_ui_graph("ui/Menu.uigraph.json"));
    assert_eq!(ui_graph_gui_name("ui/Main Menu.uigraph.json"), "Main Menu");
}
