//! The ready-made widgets: Slider, Toggle, Dropdown and ProgressBar.

use super::*;
use axcenity_render::UiRectangle;

fn screen_gui(world: &mut World) -> InstanceId {
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    gui
}

/// A `class` with `Id` `name`, at `x`, `y` pixels, `w` × `h` big.
fn element(
    world: &mut World,
    parent: InstanceId,
    class: ClassName,
    name: &str,
    [x, y, w, h]: [f32; 4],
) -> InstanceId {
    let id = world.create(class).unwrap();
    world.set_name(id, name).unwrap();
    world.set_parent(id, Some(parent)).unwrap();
    world
        .set_property(id, "Id", Value::String(name.to_owned()))
        .unwrap();
    world
        .set_property(id, "Position", Value::UDim2(UDim2::new(0.0, x, 0.0, y)))
        .unwrap();
    world
        .set_property(id, "Size", Value::UDim2(UDim2::new(0.0, w, 0.0, h)))
        .unwrap();
    id
}

fn css(source: &str) -> Vec<Stylesheet> {
    let (sheet, warnings) = parse_stylesheet("ui.css", source);
    assert!(warnings.is_empty(), "{warnings:?}");
    vec![sheet]
}

fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
    world.set_property(id, name, value).unwrap();
}

fn number(world: &World, id: InstanceId) -> f64 {
    world
        .get(id)
        .unwrap()
        .property("Value")
        .and_then(Value::as_number)
        .unwrap()
}

fn text_value(world: &World, id: InstanceId) -> String {
    world
        .get(id)
        .unwrap()
        .property("Value")
        .and_then(Value::as_str)
        .unwrap()
        .to_owned()
}

fn rectangles(layout: &UiLayout) -> Vec<UiRectangle> {
    layout
        .draw_items
        .iter()
        .filter_map(|item| match item {
            UiDrawItem::Rectangle(rectangle) if rectangle.shadow.is_none() => Some(*rectangle),
            _ => None,
        })
        .collect()
}

/// The rectangles drawn in colour `hex` (`0xrrggbb`).
fn in_colour(layout: &UiLayout, hex: u32) -> Vec<UiRectangle> {
    let rgb = [
        (hex >> 16) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
    ];
    rectangles(layout)
        .into_iter()
        .filter(|r| (0..3).all(|i| (r.color[i] - rgb[i]).abs() < 0.01) && r.color[3] > 0.0)
        .collect()
}

fn lay_out(interaction: &mut UiInteraction, world: &World, sheets: &[Stylesheet]) -> UiLayout {
    lay_out_at(interaction, world, sheets, None)
}

fn lay_out_at(
    interaction: &mut UiInteraction,
    world: &World,
    sheets: &[Stylesheet],
    pointer: Option<[f32; 2]>,
) -> UiLayout {
    let input = UiInput {
        pointer,
        ..UiInput::default()
    };
    interaction.layout(world, (800, 600), sheets, input, None, 0.0)
}

/// A click: the button down and up again at `at`, applied to `world`.
fn click(interaction: &mut UiInteraction, world: &mut World, at: [f32; 2]) -> bool {
    let taken = interaction.press(world, at).apply(world);
    let released = interaction.release(world, Some(at));
    taken | released.apply(world)
}

const ACCENT: u32 = 0x3b82f6;

#[test]
fn a_slider_picks_its_value_by_dragging_or_clicking_the_track_in_steps() {
    let mut world = World::new();
    let gui = screen_gui(&mut world);
    let volume = element(
        &mut world,
        gui,
        ClassName::Slider,
        "volume",
        [0.0, 0.0, 240.0, 24.0],
    );
    set(&mut world, volume, "Step", Value::Number(10.0));
    let mut ui = UiInteraction::default();
    let layout = lay_out(&mut ui, &world, &[]);
    // The thumb is 20 across, so its middle goes from 10 to 230.
    let track = layout.element(volume).unwrap().track.unwrap();
    assert_eq!(
        track,
        UiTrack {
            axis: 0,
            from: 10.0,
            to: 230.0
        }
    );
    // The fill runs to the thumb, at 50.
    let fill = in_colour(&layout, ACCENT);
    assert_eq!(fill.len(), 1);
    assert_eq!(fill[0].size[0], 120.0);

    // A press on the track jumps there, and dragging follows, in steps of 10.
    let press = ui.press(&world, [64.0, 12.0]);
    assert!(press.taken, "a press on a slider presses nothing under it");
    assert_eq!(
        press.value,
        Some((volume, Value::Number(20.0))),
        "54 / 220 is 24.5%"
    );
    press.apply(&mut world);
    assert!(ui.scrolling());
    assert_eq!(
        ui.drag(&world, [60.0, 12.0]).value,
        None,
        "still 20, so no change"
    );
    ui.drag(&world, [180.0, 40.0]).apply(&mut world);
    assert_eq!(number(&world, volume), 80.0);
    ui.drag(&world, [600.0, 12.0]).apply(&mut world);
    assert_eq!(number(&world, volume), 100.0, "kept in range");
    assert_eq!(ui.release(&world, Some([600.0, 12.0])).value, None);
    assert!(!ui.scrolling());

    // Setting Value (as a script does) moves the thumb.
    set(&mut world, volume, "Value", Value::Number(25.0));
    let layout = lay_out(&mut ui, &world, &[]);
    assert_eq!(in_colour(&layout, ACCENT)[0].size[0], 65.0);

    // With Step 0 any number is picked, and a vertical slider has its
    // MinValue at the bottom.
    set(&mut world, volume, "Step", Value::Number(0.0));
    set(
        &mut world,
        volume,
        "Size",
        Value::UDim2(UDim2::new(0.0, 24.0, 0.0, 240.0)),
    );
    set(
        &mut world,
        volume,
        "Orientation",
        Value::String("Vertical".into()),
    );
    lay_out(&mut ui, &world, &[]);
    ui.press(&world, [12.0, 230.0]).apply(&mut world);
    assert_eq!(number(&world, volume), 0.0);
    ui.drag(&world, [12.0, 65.0]).apply(&mut world);
    assert_eq!(number(&world, volume), 75.0);
    let _ = ui.release(&world, None);

    // Not Interactable, the pointer can't change it.
    set(&mut world, volume, "Interactable", Value::Bool(false));
    lay_out(&mut ui, &world, &[]);
    assert!(!ui.press(&world, [12.0, 230.0]).taken);
    assert_eq!(number(&world, volume), 75.0);
}

#[test]
fn a_toggle_flips_when_clicked_and_checked_styles_its_parts() {
    let mut world = World::new();
    let gui = screen_gui(&mut world);
    let fullscreen = element(
        &mut world,
        gui,
        ClassName::Toggle,
        "fullscreen",
        [0.0, 0.0, 52.0, 28.0],
    );
    let sheets = css("#fullscreen:checked::track { background-color: #22c55e }");
    let mut ui = UiInteraction::default();
    let off = lay_out(&mut ui, &world, &sheets);
    // Off: a grey track, the knob at the left.
    assert_eq!(in_colour(&off, 0x5b6170).len(), 1);
    let knob = in_colour(&off, 0xffffff)[0];
    assert_eq!(knob.position, [3.0, 3.0]);

    assert!(click(&mut ui, &mut world, [26.0, 14.0]));
    assert_eq!(
        world.get(fullscreen).unwrap().property("Value"),
        Some(&Value::Bool(true))
    );
    let on = lay_out(&mut ui, &world, &sheets);
    assert_eq!(
        in_colour(&on, 0x22c55e).len(),
        1,
        ":checked::track styles it while it's on"
    );
    assert_eq!(in_colour(&on, 0xffffff)[0].position, [27.0, 3.0]);

    // Let go somewhere else, it doesn't flip.
    ui.press(&world, [26.0, 14.0]).apply(&mut world);
    assert_eq!(ui.release(&world, Some([300.0, 300.0])).value, None);

    // A checkbox is ticked when on: two turned strokes in ::check's colour.
    set(
        &mut world,
        fullscreen,
        "Style",
        Value::String("Checkbox".into()),
    );
    let sheets = css("#fullscreen::check { background-color: #ff0000 }");
    let ticked = lay_out(&mut ui, &world, &sheets);
    let strokes = in_colour(&ticked, 0xff0000);
    assert_eq!(strokes.len(), 2);
    assert!(strokes.iter().all(|stroke| stroke.rotation_degrees != 0.0));
    assert!(click(&mut ui, &mut world, [14.0, 14.0]));
    let unticked = lay_out(&mut ui, &world, &sheets);
    assert!(in_colour(&unticked, 0xff0000).is_empty());
}

#[test]
fn a_dropdown_opens_a_list_over_everything_that_scrolls_picks_and_closes() {
    let mut world = World::new();
    let gui = screen_gui(&mut world);
    let quality = element(
        &mut world,
        gui,
        ClassName::Dropdown,
        "quality",
        [10.0, 10.0, 200.0, 36.0],
    );
    let options: Vec<String> = (1..=10).map(|n| format!("Level {n}")).collect();
    set(
        &mut world,
        quality,
        "Options",
        Value::String(options.join("\n")),
    );
    // A button laid out after it, where the list will open.
    let under = element(
        &mut world,
        gui,
        ClassName::TextButton,
        "under",
        [10.0, 60.0, 200.0, 100.0],
    );
    let sheets =
        css("#quality::option { color: #ff0000 } #quality::list { background-color: #222222 }");
    let mut ui = UiInteraction::default();
    let closed = lay_out(&mut ui, &world, &sheets);
    assert!(
        closed
            .draw_items
            .iter()
            .any(|item| matches!(item, UiDrawItem::Text(text) if text.text == "Choose…")),
        "the placeholder shows until something is chosen"
    );

    assert!(click(&mut ui, &mut world, [100.0, 28.0]));
    assert_eq!(ui.open_dropdown(), Some(quality));
    let pointer = [100.0, 60.0 + 4.0 + 36.0 * 2.5];
    let open = lay_out_at(&mut ui, &world, &sheets, Some(pointer));
    // Six rows show, below it, styled by ::list and ::option, and drawn
    // after (over) the button that comes after it.
    let rows: Vec<&UiText> = open
        .draw_items
        .iter()
        .filter_map(|item| match item {
            UiDrawItem::Text(text) if text.text.starts_with("Level") => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(rows.len(), 6);
    assert!(rows.iter().all(|row| row.color[..3] == [1.0, 0.0, 0.0]));
    let list = in_colour(&open, 0x222222)[0];
    assert_eq!(list.position, [10.0, 50.0]);
    assert_eq!(list.size, [200.0, 6.0 * 36.0 + 8.0]);
    let button_at = open
        .draw_items
        .iter()
        .position(|item| matches!(item, UiDrawItem::Text(text) if text.text == "Button"))
        .unwrap();
    let list_at = open
        .draw_items
        .iter()
        .position(
            |item| matches!(item, UiDrawItem::Rectangle(r) if r.color[..3] == list.color[..3]),
        )
        .unwrap();
    assert!(button_at < list_at, "the list is drawn over the button");
    assert_eq!(
        open.hovered, None,
        "the button under the list isn't hovered"
    );
    assert_eq!(open.cursor, UiCursor::Pointer);

    // A click on the third row picks it and closes the list; the button
    // under it isn't pressed.
    assert!(click(&mut ui, &mut world, pointer));
    assert_eq!(text_value(&world, quality), "Level 3");
    assert_eq!(ui.open_dropdown(), None);
    let picked = lay_out(&mut ui, &world, &sheets);
    assert!(
        picked
            .draw_items
            .iter()
            .any(|item| matches!(item, UiDrawItem::Text(text) if text.text == "Level 3"))
    );
    assert_eq!(picked.hit_button(100.0, 100.0), Some(under));

    // The wheel over it scrolls the long list.
    click(&mut ui, &mut world, [100.0, 28.0]);
    lay_out(&mut ui, &world, &sheets);
    assert!(ui.wheel(&world, [100.0, 100.0], [0.0, 500.0]).taken);
    let scrolled = lay_out(&mut ui, &world, &sheets);
    let shown: Vec<&str> = scrolled
        .draw_items
        .iter()
        .filter_map(|item| match item {
            UiDrawItem::Text(text) if text.text.starts_with("Level") && text.position[1] > 40.0 => {
                Some(text.text.as_str())
            }
            _ => None,
        })
        .collect();
    assert!(shown.contains(&"Level 10"), "{shown:?}");
    assert!(!shown.contains(&"Level 1"), "{shown:?}");

    // A click outside closes it without picking or pressing anything.
    let press = ui.press(&world, [600.0, 500.0]);
    assert!(press.taken);
    assert_eq!(ui.open_dropdown(), None);
    assert_eq!(ui.release(&world, Some([600.0, 500.0])).value, None);
    assert_eq!(text_value(&world, quality), "Level 3");

    // Not Interactable, it doesn't open.
    set(&mut world, quality, "Interactable", Value::Bool(false));
    lay_out(&mut ui, &world, &sheets);
    assert!(!click(&mut ui, &mut world, [100.0, 28.0]));
    assert_eq!(ui.open_dropdown(), None);
}

#[test]
fn a_progress_bar_fills_as_far_as_its_value_and_its_fill_restyles() {
    let mut world = World::new();
    let gui = screen_gui(&mut world);
    let loading = element(
        &mut world,
        gui,
        ClassName::ProgressBar,
        "loading",
        [0.0, 0.0, 200.0, 20.0],
    );
    set(&mut world, loading, "Value", Value::Number(25.0));
    let layout = layout_ui(&world, 800, 600, &[]);
    let fill = in_colour(&layout, ACCENT);
    assert_eq!(fill.len(), 1);
    assert_eq!((fill[0].position, fill[0].size), ([0.0, 0.0], [50.0, 20.0]));

    // ::fill restyles it, gradients too, and a vertical bar fills from the bottom.
    let sheets = css("#loading { border: 2px solid #000000 }
         #loading::fill { background-image: linear-gradient(#ff0000, #0000ff) }");
    set(
        &mut world,
        loading,
        "Orientation",
        Value::String("Vertical".into()),
    );
    let layout = layout_ui(&world, 800, 600, &sheets);
    let fill = in_colour(&layout, 0xff0000)[0];
    assert_eq!(fill.color_end[..3], [0.0, 0.0, 1.0]);
    assert_eq!((fill.position, fill.size), ([2.0, 14.0], [196.0, 4.0]));
    // A script setting Value refills it; empty draws no fill.
    set(&mut world, loading, "Value", Value::Number(0.0));
    let layout = layout_ui(&world, 800, 600, &sheets);
    assert!(in_colour(&layout, 0xff0000).is_empty());
}

#[test]
fn a_sliders_parts_restyle_through_their_pseudo_elements() {
    let mut world = World::new();
    let gui = screen_gui(&mut world);
    element(
        &mut world,
        gui,
        ClassName::Slider,
        "volume",
        [0.0, 0.0, 240.0, 24.0],
    );
    let sheets = css("#volume::track { background-color: #111111; height: 10px }
         #volume::fill { background-color: #ff0000 }
         #volume::thumb { background-color: #00ff00; width: 30px; border-radius: 0 }
         #volume:hover::thumb { background-color: #0000ff }");
    let mut ui = UiInteraction::default();
    let layout = lay_out(&mut ui, &world, &sheets);
    let track = in_colour(&layout, 0x111111)[0];
    assert_eq!((track.size, track.position[1]), ([240.0, 10.0], 7.0));
    assert_eq!(track.border_radius, 5.0);
    // The thumb is 30 across, so its middle goes from 15 to 225: at 50, 120.
    let thumb = in_colour(&layout, 0x00ff00)[0];
    assert_eq!((thumb.position[0], thumb.size), (105.0, [30.0, 30.0]));
    assert_eq!(thumb.border_radius, 0.0);
    assert_eq!(in_colour(&layout, 0xff0000)[0].size[0], 120.0);
    // The slider's own :hover reaches its parts.
    let hovered = lay_out_at(&mut ui, &world, &sheets, Some([120.0, 12.0]));
    assert_eq!(in_colour(&hovered, 0x0000ff).len(), 1);
}

/// An editor plugin's dock tab (Phase 54): a ScreenGui outside the tree,
/// laid out alone with `UiInteraction::only`. Its Slider drags, its
/// TextButton is hit, and its stylesheet restyles it.
#[test]
fn a_screen_gui_outside_the_tree_works_alone() {
    let mut world = World::new();
    // The game's own UI isn't drawn in the plugin's tab.
    let game = screen_gui(&mut world);
    element(
        &mut world,
        game,
        ClassName::Frame,
        "game",
        [0.0, 0.0, 50.0, 50.0],
    );
    let tab = world.create(ClassName::ScreenGui).unwrap();
    let slider = element(
        &mut world,
        tab,
        ClassName::Slider,
        "amount",
        [0.0, 0.0, 240.0, 24.0],
    );
    let apply = element(
        &mut world,
        tab,
        ClassName::TextButton,
        "apply",
        [0.0, 40.0, 100.0, 30.0],
    );
    let blue = css("#apply { background-color: #0000ff; }");
    let red = css("#apply { background-color: #ff0000; }");
    let mut ui = UiInteraction::only(tab);
    let frame = |ui: &mut UiInteraction, world: &World, sheets: &[Stylesheet]| {
        ui.layout(world, (300, 200), sheets, UiInput::default(), None, 0.0)
    };
    let layout = frame(&mut ui, &world, &blue);
    let ids: Vec<_> = layout.elements.iter().map(|element| element.id).collect();
    assert_eq!(ids, [slider, apply]);
    assert_eq!(layout.hit_button(50.0, 55.0), Some(apply));
    let button_colour = |layout: &UiLayout| {
        layout
            .draw_items
            .iter()
            .find_map(|item| match item {
                UiDrawItem::Rectangle(rect) if rect.position == [0.0, 40.0] => Some(rect.color),
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(button_colour(&layout), [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(
        button_colour(&frame(&mut ui, &world, &red)),
        [1.0, 0.0, 0.0, 1.0]
    );

    ui.press(&world, [230.0, 12.0]).apply(&mut world);
    let _ = ui.release(&world, Some([230.0, 12.0])).apply(&mut world);
    assert_eq!(number(&world, slider), 100.0);
}
