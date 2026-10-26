//! Layer order (`ZIndex`, `z-index`, `DisplayOrder`), clipping
//! (`overflow: hidden`) and ScrollingFrames.

use super::*;
use axcenity_core::Vector2;

fn screen_gui(world: &mut World) -> InstanceId {
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    gui
}

/// A `class` named `name` in `parent`, at `x`, `y` pixels, `w` × `h` big.
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
        .set_property(id, "Position", Value::UDim2(UDim2::new(0.0, x, 0.0, y)))
        .unwrap();
    world
        .set_property(id, "Size", Value::UDim2(UDim2::new(0.0, w, 0.0, h)))
        .unwrap();
    id
}

fn css(source: &str) -> Vec<Stylesheet> {
    vec![parse_stylesheet("ui.css", source).0]
}

/// Where in the draw list `id`'s first item is.
fn drawn_at(layout: &UiLayout, world: &World, id: InstanceId) -> usize {
    let rect = layout.element(id).unwrap().rect;
    layout
        .draw_items
        .iter()
        .position(|item| match item {
            UiDrawItem::Rectangle(r) => r.position == [rect.x, rect.y] && r.size[0] == rect.width,
            _ => false,
        })
        .unwrap_or_else(|| panic!("{} is drawn", world.get(id).unwrap().name()))
}

#[test]
fn a_higher_z_index_puts_a_sibling_and_its_children_on_top_and_it_takes_the_click() {
    let mut world = World::new();
    let gui = screen_gui(&mut world);
    let a = element(
        &mut world,
        gui,
        ClassName::TextButton,
        "A",
        [0.0, 0.0, 100.0, 100.0],
    );
    let b = element(
        &mut world,
        gui,
        ClassName::TextButton,
        "B",
        [50.0, 50.0, 100.0, 100.0],
    );
    let inside_a = element(
        &mut world,
        a,
        ClassName::Frame,
        "Inside",
        [60.0, 60.0, 30.0, 30.0],
    );

    // Ties keep tree order: B, later, is on top.
    let layout = layout_ui(&world, 400, 300, &[]);
    assert_eq!(layout.hit_button(75.0, 75.0), Some(b));
    assert!(drawn_at(&layout, &world, a) < drawn_at(&layout, &world, b));

    world.set_property(a, "ZIndex", Value::Number(2.0)).unwrap();
    let layout = layout_ui(&world, 400, 300, &[]);
    assert_eq!(
        layout.hit_button(55.0, 55.0),
        Some(a),
        "A on top takes the click"
    );
    assert!(drawn_at(&layout, &world, b) < drawn_at(&layout, &world, a));
    assert!(
        drawn_at(&layout, &world, b) < drawn_at(&layout, &world, inside_a),
        "A's children come up with it"
    );
    assert_eq!(layout.hit(70.0, 70.0), Some(inside_a));

    // CSS z-index wins over ZIndex.
    let sheets = css("#b { z-index: 5 }");
    world
        .set_property(b, "Id", Value::String("b".into()))
        .unwrap();
    let layout = layout_ui(&world, 400, 300, &sheets);
    assert_eq!(layout.hit_button(75.0, 75.0), Some(b));
    assert_eq!(z_index_of(&world, b, &sheets), 5.0);
    assert!(css_sets_z_index(&world, b, &sheets));
    assert!(!css_sets_z_index(&world, a, &sheets));

    // Hover follows the same order.
    let mut interaction = UiInteraction::default();
    let input = UiInput {
        pointer: Some([75.0, 75.0]),
        ..UiInput::default()
    };
    let layout = interaction.layout(&world, (400, 300), &sheets, input, None, 0.0);
    assert_eq!(layout.hovered, Some(b));
}

#[test]
fn display_order_draws_screen_guis_lowest_first_and_ties_keep_tree_order() {
    let mut world = World::new();
    let low = screen_gui(&mut world);
    let high = screen_gui(&mut world);
    let under = element(
        &mut world,
        low,
        ClassName::TextButton,
        "Low",
        [0.0, 0.0, 100.0, 100.0],
    );
    let over = element(
        &mut world,
        high,
        ClassName::TextButton,
        "High",
        [0.0, 0.0, 90.0, 90.0],
    );
    assert_eq!(
        layout_ui(&world, 200, 200, &[]).hit_button(10.0, 10.0),
        Some(over)
    );
    world
        .set_property(low, "DisplayOrder", Value::Number(1.0))
        .unwrap();
    let layout = layout_ui(&world, 200, 200, &[]);
    assert_eq!(layout.hit_button(10.0, 10.0), Some(under));
    assert!(drawn_at(&layout, &world, over) < drawn_at(&layout, &world, under));
}

#[test]
fn overflow_hidden_clips_children_to_the_rounded_box_and_where_they_take_the_pointer() {
    let mut world = World::new();
    let gui = screen_gui(&mut world);
    let frame = element(
        &mut world,
        gui,
        ClassName::Frame,
        "Card",
        [0.0, 0.0, 100.0, 100.0],
    );
    world
        .set_property(frame, "Id", Value::String("card".into()))
        .unwrap();
    let image = element(
        &mut world,
        frame,
        ClassName::ImageLabel,
        "Photo",
        [0.0, 0.0, 100.0, 100.0],
    );
    world
        .set_property(image, "Image", Value::String("photo.png".into()))
        .unwrap();
    let wide = element(
        &mut world,
        frame,
        ClassName::TextButton,
        "Wide",
        [80.0, 80.0, 60.0, 60.0],
    );

    // Without overflow, nothing is cut.
    let layout = layout_ui(&world, 300, 300, &[]);
    assert_eq!(layout.element(image).unwrap().clip, None);
    assert_eq!(layout.hit_button(120.0, 120.0), Some(wide));

    let sheets = css("#card { overflow: hidden; border-radius: 20px }");
    let layout = layout_ui(&world, 300, 300, &sheets);
    let rounded = UiClip {
        position: [0.0, 0.0],
        size: [100.0, 100.0],
        radius: 20.0,
        rotation_degrees: 0.0,
    };
    let picture = layout
        .draw_items
        .iter()
        .find_map(|item| match item {
            UiDrawItem::Image(image) => Some(image),
            _ => None,
        })
        .expect("the photo is drawn");
    assert_eq!(picture.clip, Some(rounded), "cut to the card's rounded box");
    assert_eq!(layout.element(image).unwrap().clip, Some(rounded));
    assert_eq!(layout.element(frame).unwrap().clip, None, "not cut itself");
    // The card's corner, outside its rounding, isn't the photo's.
    assert_eq!(layout.hit(2.0, 2.0), Some(frame));
    assert_eq!(layout.hit(50.0, 50.0), Some(image));
    // The part of the button outside the card takes no clicks.
    assert_eq!(layout.hit_button(120.0, 120.0), None);
    assert_eq!(layout.hit_button(90.0, 90.0), Some(wide));

    // A border's inside is the box: rounded less by its width.
    let sheets = css("#card { overflow: hidden; border-radius: 20px; border: 4px solid #000 }");
    let layout = layout_ui(&world, 300, 300, &sheets);
    assert_eq!(
        layout.element(image).unwrap().clip,
        Some(UiClip {
            position: [4.0, 4.0],
            size: [92.0, 92.0],
            radius: 16.0,
            rotation_degrees: 0.0,
        })
    );

    // On the artboard at half size the clip shrinks with everything else.
    let sheets = css("#card { overflow: hidden; border-radius: 20px }");
    let board = layout_artboard(&world, gui, &sheets, (300, 300), 0.5, [10.0, 10.0]);
    assert_eq!(
        board.element(image).unwrap().clip,
        Some(UiClip {
            position: [10.0, 10.0],
            size: [50.0, 50.0],
            radius: 10.0,
            rotation_degrees: 0.0,
        })
    );
}

/// A ScreenGui with a 200 × 200 ScrollingFrame holding twenty 50 pixel
/// tall buttons, one under the other, its canvas growing to fit them.
fn list() -> (World, InstanceId, Vec<InstanceId>) {
    let mut world = World::new();
    let gui = screen_gui(&mut world);
    let list = element(
        &mut world,
        gui,
        ClassName::ScrollingFrame,
        "List",
        [0.0, 0.0, 200.0, 200.0],
    );
    world
        .set_property(list, "Id", Value::String("list".into()))
        .unwrap();
    world
        .set_property(
            list,
            "CanvasSize",
            Value::UDim2(UDim2::new(0.0, 0.0, 0.0, 0.0)),
        )
        .unwrap();
    world
        .set_property(list, "AutomaticCanvasSize", Value::String("Y".into()))
        .unwrap();
    let buttons = (0..20)
        .map(|i| {
            let y = i as f32 * 50.0;
            element(
                &mut world,
                list,
                ClassName::TextButton,
                &format!("B{i}"),
                [0.0, y, 180.0, 50.0],
            )
        })
        .collect();
    (world, list, buttons)
}

fn canvas_position(world: &World, id: InstanceId) -> [f32; 2] {
    let at = world
        .get(id)
        .unwrap()
        .property("CanvasPosition")
        .and_then(Value::as_vector2)
        .unwrap();
    [at.x, at.y]
}

fn lay_out(interaction: &mut UiInteraction, world: &World) -> UiLayout {
    interaction.layout(world, (800, 600), &[], UiInput::default(), None, 0.0)
}

#[test]
fn a_scrolling_frame_scrolls_by_wheel_and_bar_and_only_its_visible_buttons_take_clicks() {
    let (mut world, list, buttons) = list();
    let mut interaction = UiInteraction::default();
    let layout = lay_out(&mut interaction, &world);
    let scroll = layout.element(list).unwrap().scroll.unwrap();
    assert_eq!(
        scroll.canvas,
        [200.0, 1000.0],
        "the canvas fits the twenty buttons"
    );
    assert_eq!(scroll.scrolls, [false, true]);
    assert_eq!(scroll.max(), [0.0, 800.0]);
    assert_eq!(layout.hit_button(10.0, 25.0), Some(buttons[0]));
    // B4 is laid out at 200 to 250, under the frame: cut off, so not clickable.
    assert_eq!(layout.hit_button(10.0, 225.0), None);

    // The wheel scrolls it down 120 pixels.
    let wheel = interaction.wheel(&world, [100.0, 100.0], [0.0, 120.0]);
    assert!(wheel.taken, "the camera doesn't zoom");
    assert!(wheel.apply(&mut world));
    assert_eq!(canvas_position(&world, list), [0.0, 120.0]);
    let layout = lay_out(&mut interaction, &world);
    assert_eq!(
        layout.hit_button(10.0, 25.0),
        Some(buttons[2]),
        "145 on the canvas"
    );
    assert_eq!(layout.hit_button(10.0, 195.0), Some(buttons[6]));
    // The wheel isn't the list's off it.
    assert!(!interaction.wheel(&world, [500.0, 500.0], [0.0, 40.0]).taken);

    // The bar: a press low on its track jumps the thumb there, and dragging
    // it to the bottom scrolls to the end.
    let element = *layout.element(list).unwrap();
    let bars = scroll_bars(element.rect, &element.scroll.unwrap(), 1.0);
    assert_eq!(bars.len(), 1);
    let bar = bars[0];
    assert_eq!(bar.axis, 1);
    assert_eq!(
        [bar.track.x, bar.track.width, bar.track.height],
        [188.0, 12.0, 200.0]
    );
    assert_eq!(
        bar.thumb.height, 40.0,
        "a fifth of the track: the window is a fifth of the canvas"
    );
    let press = interaction.press(&world, [194.0, 190.0]);
    assert!(
        press.taken,
        "the bar has the press, not the button under it"
    );
    press.apply(&mut world);
    assert!(canvas_position(&world, list)[1] > 700.0);
    assert!(interaction.scrolling());
    interaction.drag(&world, [194.0, 400.0]).apply(&mut world);
    assert_eq!(canvas_position(&world, list), [0.0, 800.0]);
    interaction.drag(&world, [194.0, 20.0]).apply(&mut world);
    assert_eq!(canvas_position(&world, list), [0.0, 0.0]);
    let _ = interaction.release(&world, None);
    assert!(!interaction.scrolling());

    // Dragging the content scrolls once it has moved far enough.
    lay_out(&mut interaction, &world);
    assert!(
        !interaction.press(&world, [100.0, 100.0]).taken,
        "a button may still be pressed"
    );
    assert!(!interaction.drag(&world, [100.0, 98.0]).taken);
    let dragged = interaction.drag(&world, [100.0, 60.0]);
    assert!(dragged.taken, "now it scrolls, so the button won't fire");
    dragged.apply(&mut world);
    assert_eq!(canvas_position(&world, list), [0.0, 40.0]);
    let _ = interaction.release(&world, None);
}

#[test]
fn a_scrolling_frame_keeps_its_canvas_position_in_range_and_to_its_direction() {
    let (mut world, list, _) = list();
    world
        .set_property(
            list,
            "CanvasPosition",
            Value::Vector2(Vector2::new(50.0, 5000.0)),
        )
        .unwrap();
    let layout = layout_ui(&world, 800, 600, &[]);
    assert_eq!(
        layout.element(list).unwrap().scroll.unwrap().position,
        [0.0, 800.0]
    );

    world
        .set_property(list, "ScrollingDirection", Value::String("X".into()))
        .unwrap();
    let mut interaction = UiInteraction::default();
    let layout = lay_out(&mut interaction, &world);
    let scroll = layout.element(list).unwrap().scroll.unwrap();
    assert_eq!(
        scroll.scrolls,
        [false, false],
        "the canvas is no wider than the frame"
    );
    assert_eq!(scroll.position, [0.0, 0.0]);
    let wheel = interaction.wheel(&world, [100.0, 100.0], [0.0, 120.0]);
    assert_eq!(wheel.moved, None);
    assert!(wheel.taken, "still over the list");
}

#[test]
fn scroll_bars_are_styled_with_scrollbar_and_thumb_rules_that_leave_the_frame_alone() {
    let (world, _, _) = list();
    let sheets = css("#list { background-color: #00ff00 }
         #list::scrollbar { background-color: #0000ff }
         #list::thumb { background-color: #ff0000; border-radius: 2px }");
    let layout = layout_ui(&world, 800, 600, &sheets);
    let rectangles: Vec<UiRectangle> = layout
        .draw_items
        .iter()
        .filter_map(|item| match item {
            UiDrawItem::Rectangle(rectangle) => Some(*rectangle),
            _ => None,
        })
        .collect();
    let frame = rectangles[0];
    assert_eq!(
        frame.color[..3],
        [0.0, 1.0, 0.0],
        "the frame is green, not red"
    );
    let track = rectangles
        .iter()
        .find(|r| r.color[..3] == [0.0, 0.0, 1.0])
        .expect("a blue track");
    assert_eq!(track.size, [12.0, 200.0]);
    let thumb = rectangles
        .iter()
        .find(|r| r.color[..3] == [1.0, 0.0, 0.0])
        .expect("a red thumb");
    assert_eq!(thumb.border_radius, 2.0);
    let last = rectangles.last().unwrap();
    assert_eq!(
        last.color, thumb.color,
        "the thumb is drawn over the buttons"
    );
    // A pseudo-element ends its selector.
    let (_, warnings) = parse_stylesheet("a.css", "#list::thumb:hover { color: red }");
    assert!(warnings[0].message.contains("unsupported selector"));
}

#[test]
fn the_designer_shows_a_frame_scrolled_without_changing_its_canvas_position() {
    let (world, list, buttons) = list();
    let gui = world.get(list).unwrap().parent().unwrap();
    let scrolled = HashMap::from([(list, [0.0, 300.0])]);
    let board = layout_artboard_view(
        &world,
        gui,
        &[],
        ArtboardView {
            screen: (800, 600),
            scale: 1.0,
            origin: [0.0, 0.0],
            shown: None,
            scrolled: Some(&scrolled),
            safe_insets: [0.0; 4],
        },
    );
    assert_eq!(
        board.element(list).unwrap().scroll.unwrap().position,
        [0.0, 300.0]
    );
    assert_eq!(board.hit_button(10.0, 25.0), Some(buttons[6]));
    assert_eq!(canvas_position(&world, list), [0.0, 0.0]);
}
