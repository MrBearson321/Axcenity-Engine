//! The ProximityPrompt's default UI (Phase 44).

use super::*;

fn css(source: &str) -> Vec<Stylesheet> {
    let (sheet, warnings) = parse_stylesheet("ui.css", source);
    assert!(warnings.is_empty(), "{warnings:?}");
    vec![sheet]
}

/// A prompt "Open · Door" on a part at the origin.
fn door(world: &mut World) -> InstanceId {
    let door = world.create(ClassName::Part).unwrap();
    world.set_parent(door, Some(world.workspace())).unwrap();
    let prompt = world.create(ClassName::ProximityPrompt).unwrap();
    world.set_parent(prompt, Some(door)).unwrap();
    for (name, text) in [("ActionText", "Open"), ("ObjectText", "Door")] {
        world
            .set_property(prompt, name, Value::String(text.to_owned()))
            .unwrap();
    }
    prompt
}

/// A camera 10 studs back from the origin, looking at it.
fn camera() -> Camera {
    Camera {
        position: Vec3::new(0.0, 0.0, 10.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 70.0,
    }
}

fn view(prompt: InstanceId, progress: f32, key: &str) -> PromptView {
    PromptView {
        prompt,
        position: [0.0, 0.0, 0.0],
        progress,
        key: key.to_owned(),
    }
}

fn texts(items: &[UiDrawItem]) -> Vec<&UiText> {
    items
        .iter()
        .filter_map(|item| match item {
            UiDrawItem::Text(text) => Some(text),
            _ => None,
        })
        .collect()
}

fn rectangles(items: &[UiDrawItem]) -> Vec<&UiRectangle> {
    items
        .iter()
        .filter_map(|item| match item {
            UiDrawItem::Rectangle(rectangle) => Some(rectangle),
            _ => None,
        })
        .collect()
}

#[test]
fn a_prompt_shows_its_key_and_action_text_dot_object_text_over_its_part() {
    let mut world = World::new();
    let prompt = door(&mut world);
    let items = prompt_items(&world, &[view(prompt, 0.0, "E")], &[], &camera(), 800, 600);
    let texts = texts(&items);
    let shown: Vec<&str> = texts.iter().map(|text| text.text.as_str()).collect();
    assert_eq!(shown, ["E", "Open · Door"]);
    // Centred on the middle of the view, where the part is.
    let background = rectangles(&items)[0];
    let middle = background.position[0] + background.size[0] * 0.5;
    assert!((middle - 400.0).abs() < 0.5, "{middle}");
    assert!(background.position[1] < 300.0 && background.position[1] + background.size[1] > 300.0);

    // With no ObjectText, ActionText alone; behind the camera, nothing.
    world
        .set_property(prompt, "ObjectText", Value::String(String::new()))
        .unwrap();
    assert_eq!(prompt_text(&world, prompt), "Open");
    let behind = PromptView {
        position: [0.0, 0.0, 20.0],
        ..view(prompt, 0.0, "E")
    };
    assert!(prompt_items(&world, &[behind], &[], &camera(), 800, 600).is_empty());
}

#[test]
fn css_styles_the_prompt_its_key_and_its_ring_which_fills_with_the_hold() {
    let mut world = World::new();
    let prompt = door(&mut world);
    let sheets = css(
        "ProximityPrompt { background-color: #ff0000; color: #00ff00; }
         ProximityPrompt::key { background-color: #0000ff; color: #ffff00; }
         ProximityPrompt::ring { color: #ff00ff; background-color: #00ffff; width: 4px; }",
    );
    let colour = |items: &[UiDrawItem], rgb: [f32; 3]| {
        rectangles(items)
            .into_iter()
            .filter(|r| r.color[..3] == rgb)
            .count()
    };
    let half = prompt_items(
        &world,
        &[view(prompt, 0.5, "X")],
        &sheets,
        &camera(),
        800,
        600,
    );
    assert_eq!(colour(&half, [1.0, 0.0, 0.0]), 1, "the box");
    assert_eq!(colour(&half, [0.0, 0.0, 1.0]), 1, "the key cap");
    assert_eq!(colour(&half, [0.0, 1.0, 1.0]), 48, "the whole ring's track");
    assert_eq!(colour(&half, [1.0, 0.0, 1.0]), 24, "half the ring held");
    let ring = rectangles(&half)
        .into_iter()
        .find(|r| r.color[..3] == [1.0, 0.0, 1.0])
        .unwrap();
    assert_eq!(ring.size[1], 4.0);
    let texts = texts(&half);
    assert_eq!(texts[0].text, "X");
    assert_eq!(texts[0].color[..3], [1.0, 1.0, 0.0]);
    assert_eq!(texts[1].color[..3], [0.0, 1.0, 0.0]);

    let held = prompt_items(
        &world,
        &[view(prompt, 1.0, "X")],
        &sheets,
        &camera(),
        800,
        600,
    );
    assert_eq!(colour(&held, [1.0, 0.0, 1.0]), 48);
    let none = prompt_items(
        &world,
        &[view(prompt, 0.0, "X")],
        &sheets,
        &camera(),
        800,
        600,
    );
    assert_eq!(colour(&none, [1.0, 0.0, 1.0]), 0);
}
