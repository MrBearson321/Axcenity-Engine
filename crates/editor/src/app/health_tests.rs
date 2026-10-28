//! Phase 43 in the editor: Insert > Health, Damage Zone and UI > Health
//! bar, and the bars following Health in Play with no script.

use axcenity_core::Value;
use axcenity_render::glam::Vec3;

use super::tests::{copy_of_example, inserted, part};
use super::*;

const FRAME: f64 = 1.0 / 60.0;

fn number(world: &World, id: InstanceId, name: &str) -> f64 {
    world
        .get(id)
        .unwrap()
        .property(name)
        .and_then(Value::as_number)
        .unwrap()
}

fn child(world: &World, parent: InstanceId, name: &str) -> InstanceId {
    world.find_first_child(parent, name).unwrap()
}

#[test]
fn inserted_health_bars_follow_the_nearest_health_with_no_script() {
    // hello-seat has a character but no Health and no stylesheet.
    let folder = copy_of_example("hello-seat", "insert-health-bar");
    let mut app = EditorApp::new(Some(&folder));
    app.viewport
        .look_at(Vec3::new(0.0, 10.0, 20.0), Vec3::new(0.0, 0.0, 0.0));
    app.snap.move_on = false;
    let character = part(&app, "Character");
    let body = child(&app.world, character, "Body");

    // A Health goes into the Model of the selected part.
    let health = inserted(&mut app, Insertable::Health, Some(body));
    assert_eq!(app.world.get(health).unwrap().parent(), Some(character));
    // A zone around the character, with nothing selected.
    app.selected = None;
    let zone = inserted(&mut app, Insertable::DamageZone, None);
    assert_eq!(app.world.get(zone).unwrap().class(), ClassName::DamageZone);

    app.selected = None;
    app.act(Action::Insert {
        what: Insertable::HealthBar,
        near: None,
    });
    assert_eq!(app.history.undo_label(), Some("Insert Health bar"));
    let billboard = child(&app.world, body, "HealthBillboard");
    let hud = part(&app, "HealthHud");
    let bars = [
        child(&app.world, billboard, "HealthBar"),
        child(&app.world, hud, "HealthBar"),
    ];
    for id in bars {
        let bar = app.world.get(id).unwrap();
        assert_eq!(bar.class(), ClassName::ProgressBar);
        assert_eq!(
            bar.property("Class").and_then(Value::as_str),
            Some("health-bar")
        );
        let binding = app.world.get(child(&app.world, id, "Health")).unwrap();
        assert_eq!(binding.class(), ClassName::ObjectValue);
        assert_eq!(
            binding.property("Value"),
            Some(&Value::Instance(Some(health)))
        );
    }
    // Styled by a new stylesheet, which the same undo step removes.
    let css = std::fs::read_to_string(folder.join("health-bar.css")).unwrap();
    assert!(css.contains(".health-bar::fill"), "{css}");

    // In Play the zone burns the character and both bars follow, with no
    // script.
    app.play();
    for _ in 0..60 {
        app.play_frame(FRAME, &[], None, None);
    }
    let game = app.play.as_ref().unwrap().world();
    let now = number(game, health, "Health");
    assert!(now < 95.0, "{now}");
    for bar in bars {
        assert_eq!(number(game, bar, "Value"), now);
        assert_eq!(number(game, bar, "MaxValue"), 100.0);
    }
    app.stop();

    // Undo takes the bars and the stylesheet away together.
    app.undo();
    assert!(
        app.world
            .find_first_child(app.world.workspace(), "HealthHud")
            .is_none()
    );
    assert!(
        app.world
            .find_first_child(body, "HealthBillboard")
            .is_none()
    );
    assert!(!folder.join("health-bar.css").exists());
}
