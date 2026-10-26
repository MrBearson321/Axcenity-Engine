//! NavigationAgents in the editor: inserting one, walking it to a
//! Destination set in Properties during Play, and the Agent Paths overlay.

use axcenity_core::{Value, Vector3};
use axcenity_render::Camera;
use axcenity_render::glam::Vec3;

use super::tests::{copy_of_example, inserted, play_for, printed, property, vector};
use super::*;
use crate::screen::Screen;

fn feet(world: &World, body: InstanceId) -> Vector3 {
    axcenity_nav::feet_of(world, &[body]).unwrap()
}

fn near(a: Vector3, x: f32, z: f32) -> bool {
    (a.x - x).hypot(a.z - z) < 1e-3
}

/// hello-agent with a new Agent inserted from the toolbar with nothing
/// selected, its Body moved to (-20, 0, -20) and its Destination set to
/// (20, 0, -20), behind the wall, both from Properties.
fn agent_sent_round_the_wall(test: &str) -> (EditorApp, InstanceId, InstanceId) {
    let folder = copy_of_example("hello-agent", test);
    let mut app = EditorApp::new(Some(&folder));
    let agent = inserted(&mut app, Insertable::NavigationAgent, None);
    let model = app.world.get(agent).unwrap().parent().unwrap();
    let body = app.world.find_first_child(model, "Body").unwrap();
    app.apply(body, Edit::Property("Position", vector(-20.0, 2.5, -20.0)));
    app.apply(
        agent,
        Edit::Property("Destination", vector(20.0, 0.0, -20.0)),
    );
    (app, agent, body)
}

#[test]
fn an_inserted_agent_starts_where_it_stands_and_stays_there_in_play() {
    let folder = copy_of_example("hello-agent", "agent-insert");
    let mut app = EditorApp::new(Some(&folder));
    let agent = inserted(&mut app, Insertable::NavigationAgent, None);
    let model = app.world.get(agent).unwrap().parent().unwrap();
    assert_eq!(app.world.get(model).unwrap().name(), "Agent");
    let body = app.world.find_first_child(model, "Body").unwrap();
    let start = feet(&app.world, body);
    assert_eq!(property(&app, agent, "Destination"), Value::Vector3(start));

    // Into a Model that already has a body, it goes in as it is.
    let guard = app
        .world
        .find_first_child(app.world.workspace(), "Guard")
        .unwrap();
    let second = inserted(&mut app, Insertable::NavigationAgent, Some(guard));
    assert_eq!(app.world.get(second).unwrap().parent(), Some(guard));

    app.play();
    play_for(&mut app, 0.5);
    let game = shown(&app.world, &app.play);
    assert_eq!(feet(game, body), start);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn an_agent_walks_round_the_wall_to_a_destination_set_in_properties() {
    let (mut app, agent, body) = agent_sent_round_the_wall("agent-walk");
    app.play();
    // The wall runs from Z = -24 to 12; its -Z end is nearer.
    let mut nearest_z = f32::INFINITY;
    for _ in 0..8 * 60 {
        play_for(&mut app, 1.0 / 60.0);
        let at = feet(shown(&app.world, &app.play), body);
        nearest_z = nearest_z.min(at.z);
        let clear = at.x.abs() > 1.5 || at.z > 12.0 || at.z < -24.0;
        assert!(clear, "in the wall at {at:?}");
    }
    let game = shown(&app.world, &app.play);
    assert!(
        near(feet(game, body), 20.0, -20.0),
        "{:?}",
        feet(game, body)
    );
    assert!(nearest_z < -24.0, "went round the wall's end");
    let status = game.get(agent).unwrap().property("Status").cloned();
    assert_eq!(status, Some(Value::String("Reached".to_owned())));

    // Stop puts the edit world back: the Body where it was put.
    app.stop();
    assert!(near(feet(&app.world, body), -20.0, -20.0));
}

#[test]
fn an_unreachable_destination_is_reported_in_output_and_the_agent_stays() {
    let (mut app, agent, body) = agent_sent_round_the_wall("agent-unreachable");
    app.apply(agent, Edit::Property("Destination", vector(0.0, 0.0, 50.0)));
    app.play();
    play_for(&mut app, 1.0);
    let game = shown(&app.world, &app.play);
    assert!(near(feet(game, body), -20.0, -20.0));
    let status = game.get(agent).unwrap().property("Status").cloned();
    assert_eq!(status, Some(Value::String("Unreachable".to_owned())));
    let reported: Vec<&str> = printed(&app)
        .into_iter()
        .filter(|line| line.contains("can't reach"))
        .collect();
    assert_eq!(
        reported,
        vec![
            "Workspace.Agent.NavigationAgent can't reach (0, 0, 50): no walkable route leads \
             there."
        ]
    );
    let line = app
        .output
        .lines()
        .iter()
        .find(|line| line.text.contains("can't reach"))
        .unwrap();
    assert_eq!(line.kind, crate::output::OutputKind::Warning);
}

#[test]
fn drawing_agent_paths_does_not_change_the_route() {
    let ctx = theme::test_context();
    let camera = Camera {
        position: Vec3::new(0.0, 80.0, 60.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 60.0,
    };
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0));
    let screen = Screen::new(camera, rect);

    let (mut drawn, _, body) = agent_sent_round_the_wall("agent-drawn");
    let (mut plain, _, _) = agent_sent_round_the_wall("agent-plain");
    drawn.overlays.agent_paths = true;
    drawn.play();
    plain.play();
    let mut lines = 0;
    for _ in 0..6 * 60 {
        // The overlay reads the paths and draws them every frame, as the
        // viewport does, in one game and not the other.
        let paths = drawn.play.as_ref().unwrap().agent_paths();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            crate::viewport::paint_agent_paths(&painter, &screen, &paths);
        });
        lines += output
            .shapes
            .iter()
            .filter(|shape| matches!(shape.shape, egui::Shape::LineSegment { .. }))
            .count();
        play_for(&mut drawn, 1.0 / 60.0);
        play_for(&mut plain, 1.0 / 60.0);
        let (a, b) = (
            feet(shown(&drawn.world, &drawn.play), body),
            feet(shown(&plain.world, &plain.play), body),
        );
        assert_eq!(a, b, "the same route, step for step");
    }
    assert!(lines > 0, "the path was drawn");
}

#[test]
fn the_path_overlay_draws_a_line_through_each_corner_and_a_ring_at_the_end() {
    let ctx = theme::test_context();
    let camera = Camera {
        position: Vec3::new(0.0, 80.0, 60.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 60.0,
    };
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0));
    let screen = Screen::new(camera, rect);
    let path = vec![
        Vector3::new(-20.0, 0.0, -20.0),
        Vector3::new(-2.5, 0.0, 13.5),
        Vector3::new(20.0, 0.0, -20.0),
    ];
    let paths = vec![(World::new().workspace(), path.clone())];
    let output = ctx.run(egui::RawInput::default(), |ctx| {
        let painter = ctx.layer_painter(egui::LayerId::background());
        crate::viewport::paint_agent_paths(&painter, &screen, &paths);
    });
    let lifted = |p: Vector3| Vec3::new(p.x, p.y + 0.1, p.z);
    let segments: Vec<[egui::Pos2; 2]> = output
        .shapes
        .iter()
        .filter_map(|shape| match shape.shape {
            egui::Shape::LineSegment { points, .. } => Some(points),
            _ => None,
        })
        .collect();
    let expected: Vec<[egui::Pos2; 2]> = path
        .windows(2)
        .map(|pair| screen.line(lifted(pair[0]), lifted(pair[1])).unwrap())
        .collect();
    assert_eq!(segments, expected);
    let circles: Vec<(egui::Pos2, bool)> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Circle(circle) => {
                Some((circle.center, circle.fill != egui::Color32::TRANSPARENT))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        circles,
        vec![
            (screen.point(lifted(path[1])).unwrap(), true),
            (screen.point(lifted(path[2])).unwrap(), false),
        ],
        "a dot on the corner, a ring at the end"
    );
}

#[test]
fn sight_range_set_in_properties_changes_what_the_guard_sees_in_play_and_saves() {
    let folder = copy_of_example("hello-perception", "agent-senses");
    let mut app = EditorApp::new(Some(&folder));
    let workspace = app.world.workspace();
    let guard = app.world.find_first_child(workspace, "Guard").unwrap();
    let agent = app
        .world
        .find_first_child(guard, "NavigationAgent")
        .unwrap();
    let character = app.world.find_first_child(workspace, "Character").unwrap();
    let body = app.world.find_first_child(character, "Body").unwrap();
    // Out in the open, 15 studs to the side of the wall and 25 away.
    app.apply(body, Edit::Property("Position", vector(-15.0, 2.5, 20.0)));
    let seen = |app: &EditorApp| {
        printed(app)
            .iter()
            .filter(|line| **line == "Guard: I see you!")
            .count()
    };

    app.play();
    play_for(&mut app, 0.5);
    assert_eq!(seen(&app), 1, "{:?}", printed(&app));
    let game = shown(&app.world, &app.play);
    let sees = game.get(agent).unwrap().property("SeesPlayer").cloned();
    assert_eq!(sees, Some(Value::Bool(true)));
    app.stop();
    // Stop puts the edit world back.
    assert_eq!(property(&app, agent, "SeesPlayer"), Value::Bool(false));

    // A SightRange shorter than the player is away: not seen.
    app.apply(agent, Edit::Property("SightRange", Value::Number(20.0)));
    app.play();
    play_for(&mut app, 0.5);
    assert_eq!(seen(&app), 1, "no new sighting: {:?}", printed(&app));
    app.stop();

    // It saves with the agent.
    assert!(app.save());
    let reopened = EditorApp::new(Some(&folder));
    let guard = reopened
        .world
        .find_first_child(reopened.world.workspace(), "Guard")
        .unwrap();
    let agent = reopened
        .world
        .find_first_child(guard, "NavigationAgent")
        .unwrap();
    assert_eq!(
        property(&reopened, agent, "SightRange"),
        Value::Number(20.0)
    );
    let _ = std::fs::remove_dir_all(&folder);
}
