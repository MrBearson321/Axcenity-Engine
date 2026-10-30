//! UI drawing: layers, sliced images, borders and shadows, clips and turned
//! items, checked on the GPU where it matters.

use super::*;

#[test]
fn ui_that_does_not_overlap_is_one_layer() {
    let items = [
        ui_rect([0.0, 0.0], [100.0, 40.0]),
        ui_label([0.0, 0.0], [100.0, 40.0]),
        ui_rect([300.0, 300.0], [100.0, 40.0]),
        ui_label([300.0, 300.0], [100.0, 40.0]),
    ];
    assert_eq!(ui_layers(&items), vec![0..4]);
}

#[test]
fn a_rectangle_over_earlier_text_starts_a_layer() {
    // A HUD label, then a full-screen loading screen with its own caption.
    let items = [
        ui_rect([10.0, 10.0], [200.0, 40.0]),
        ui_label([10.0, 10.0], [200.0, 40.0]),
        ui_rect([0.0, 0.0], [1280.0, 720.0]),
        ui_label([500.0, 400.0], [280.0, 30.0]),
    ];
    assert_eq!(ui_layers(&items), vec![0..2, 2..4]);
    assert!(ui_layers(&[]).is_empty());
}

#[test]
fn text_shadows_count_toward_overlap() {
    let mut label = UiText {
        position: [0.0, 0.0],
        size: [100.0, 20.0],
        ..UiText::default()
    };
    label.shadows.push(crate::draw_list::TextShadow {
        offset: [0.0, 12.0],
        blur: 0.0,
        color: [0.0, 0.0, 0.0, 1.0],
    });
    // Just under the box, but the shadow reaches it.
    let items = [UiDrawItem::Text(label), ui_rect([0.0, 26.0], [100.0, 10.0])];
    assert_eq!(ui_layers(&items), vec![0..1, 1..2]);
}

#[test]
fn a_sliced_image_keeps_its_corners_whatever_its_size() {
    let slice = crate::UiSlice {
        insets: [10.0; 4],
        widths: [10.0; 4],
        fill: true,
        repeat: false,
    };
    for [w, h] in [[100.0, 100.0], [600.0, 200.0]] {
        let pieces = super::super::ui_pass::slice_pieces(&slice, [0.0, 0.0, w, h], [30.0, 30.0]);
        assert_eq!(pieces.len(), 9);
        // The top left corner is 10 × 10 and shows the texture's corner,
        // up to half a texel inside the cut.
        assert_eq!(
            pieces[0],
            ([0.0, 0.0, 10.0, 10.0], [0.0, 0.0, 9.5 / 30.0, 9.5 / 30.0])
        );
        // The bottom right one too, at the far corner.
        assert_eq!(pieces[8].0, [w - 10.0, h - 10.0, 10.0, 10.0]);
        // The middle stretches over the rest.
        assert_eq!(pieces[4].0, [10.0, 10.0, w - 20.0, h - 20.0]);
    }
    // Without fill there's no middle; repeating tiles the edges at the
    // corners' scale, the last tile cut short.
    let tiled = crate::UiSlice {
        fill: false,
        repeat: true,
        ..slice
    };
    let pieces = super::super::ui_pass::slice_pieces(&tiled, [0.0, 0.0, 45.0, 30.0], [30.0, 30.0]);
    let top: Vec<_> = pieces.iter().filter(|(rect, _)| rect[1] == 0.0).collect();
    assert_eq!(
        top.iter().map(|(rect, _)| rect[2]).collect::<Vec<_>>(),
        [10.0, 10.0, 10.0, 5.0, 10.0]
    );
    assert!(
        !pieces
            .iter()
            .any(|(rect, _)| rect[0] == 10.0 && rect[1] == 10.0)
    );
    // Corners too big for the box shrink together.
    let pieces = super::super::ui_pass::slice_pieces(&slice, [0.0, 0.0, 10.0, 40.0], [30.0, 30.0]);
    assert_eq!(pieces[0].0, [0.0, 0.0, 5.0, 5.0]);
}

/// The pixels of a `width` × `height` frame showing only `ui` over black.
fn ui_pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    ui: Vec<UiDrawItem>,
    (width, height): (u32, u32),
) -> impl Fn(u32, u32) -> [u8; 4] {
    let mut list = list_of(Vec::new());
    list.ui = ui;
    let mut renderer = TextureRenderer::new(device, queue, width, height);
    renderer.render(&list).unwrap();
    let pixels = renderer.read_pixels();
    move |x, y| {
        pixels[((y * width + x) * 4) as usize..][..4]
            .try_into()
            .unwrap()
    }
}

#[test]
fn a_nine_slice_frame_draws_its_corners_unstretched_on_the_gpu() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    // 30 × 30: red corners, green edges and a blue middle, 10 pixels each.
    let texture = Arc::new(Painted::new(30, |x, y| {
        match ((x / 10) == 1, (y / 10) == 1) {
            (false, false) => [255, 0, 0, 255],
            (true, true) => [0, 0, 255, 255],
            _ => [0, 255, 0, 255],
        }
    }));
    let image = |position: [f32; 2], size: [f32; 2]| {
        UiDrawItem::Image(crate::UiImage {
            position,
            size,
            tint: [1.0; 4],
            border_radius: 0.0,
            path: "frame.png".to_owned(),
            texture: Some(texture.clone()),
            slice: Some(crate::UiSlice {
                insets: [10.0; 4],
                widths: [10.0; 4],
                fill: true,
                repeat: false,
            }),
            clip: None,
            rotation_degrees: 0.0,
        })
    };
    let red = [255, 0, 0, 255];
    let green = [0, 255, 0, 255];
    let blue = [0, 0, 255, 255];
    for [w, h] in [[100u32, 100u32], [600, 200]] {
        let pixel = ui_pixels(
            &device,
            &queue,
            vec![image([20.0, 20.0], [w as f32, h as f32])],
            (w + 40, h + 40),
        );
        for (x, y) in [(23, 23), (w + 16, 23), (23, h + 16), (w + 16, h + 16)] {
            assert_eq!(pixel(x, y), red, "a corner at {x}, {y} on {w} × {h}");
        }
        // Just past a 10 pixel corner is an edge, at either size.
        assert_eq!(pixel(33, 23), green, "the top edge on {w} × {h}");
        assert_eq!(pixel(23, 33), green, "the left edge on {w} × {h}");
        assert_eq!(
            pixel(20 + w / 2, 20 + h / 2),
            blue,
            "the middle on {w} × {h}"
        );
    }
}

#[test]
fn borders_and_inset_shadows_keep_inside_rounded_corners_on_the_gpu() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let side = |color: [f32; 4]| crate::UiBorderSide { width: 6.0, color };
    let red = [1.0, 0.0, 0.0, 1.0];
    let blue = [0.0, 0.0, 1.0, 1.0];
    let rounded = UiRectangle {
        position: [8.0, 8.0],
        size: [48.0, 48.0],
        color: [0.5, 0.5, 0.5, 1.0],
        color_end: [0.5, 0.5, 0.5, 1.0],
        border_radius: 16.0,
        ..UiRectangle::default()
    };
    // An inset-style border: lit top and left, shaded bottom and right.
    let bordered = UiRectangle {
        border_sides: Some([side(red), side(blue), side(blue), side(red)]),
        ..rounded
    };
    // A ridge's inner half, over it.
    let inner = UiRectangle {
        position: [11.0, 11.0],
        size: [42.0, 42.0],
        color: [0.0; 4],
        color_end: [0.0; 4],
        border_radius: 13.0,
        border_sides: Some(
            [crate::UiBorderSide {
                width: 3.0,
                color: blue,
            }; 4],
        ),
        ..UiRectangle::default()
    };
    let shadow = UiRectangle {
        color: [1.0, 1.0, 1.0, 1.0],
        color_end: [1.0, 1.0, 1.0, 1.0],
        shadow: Some(crate::UiShadow {
            offset: [6.0, 6.0],
            blur: 0.0,
            spread: 0.0,
            inset: true,
        }),
        ..rounded
    };
    let black = [0, 0, 0, 255];
    for (name, items) in [
        ("border", vec![UiDrawItem::Rectangle(bordered)]),
        (
            "ridge",
            vec![
                UiDrawItem::Rectangle(bordered),
                UiDrawItem::Rectangle(inner),
            ],
        ),
        (
            "inset shadow",
            vec![
                UiDrawItem::Rectangle(rounded),
                UiDrawItem::Rectangle(shadow),
            ],
        ),
    ] {
        let pixel = ui_pixels(&device, &queue, items, (64, 64));
        // The square's corner, outside its rounding, stays clear.
        for (x, y) in [(9, 9), (54, 9), (9, 54), (54, 54)] {
            assert_eq!(pixel(x, y), black, "{name}: the corner at {x}, {y}");
        }
        // The middle of the top edge has the border or shadow.
        assert_ne!(pixel(32, 9), black, "{name}: the top edge");
    }
    let pixel = ui_pixels(
        &device,
        &queue,
        vec![UiDrawItem::Rectangle(bordered)],
        (64, 64),
    );
    assert_eq!(pixel(32, 9)[..3], [255, 0, 0], "lit top");
    assert_eq!(pixel(54, 32)[..3], [0, 0, 255], "shaded right");
    // The inset shadow lights the top and left edges only.
    let pixel = ui_pixels(
        &device,
        &queue,
        vec![
            UiDrawItem::Rectangle(rounded),
            UiDrawItem::Rectangle(shadow),
        ],
        (64, 64),
    );
    assert_eq!(pixel(32, 10)[..3], [255, 255, 255], "the shadow's top");
    assert_ne!(
        pixel(32, 53)[..3],
        [255, 255, 255],
        "no shadow at the bottom"
    );
}

#[test]
fn an_outer_shadow_with_blur_reaches_past_its_box_and_counts_toward_overlap() {
    let shadow = UiDrawItem::Rectangle(UiRectangle {
        position: [0.0, 0.0],
        size: [20.0, 20.0],
        color: [0.0, 0.0, 0.0, 0.5],
        shadow: Some(crate::UiShadow {
            offset: [0.0, 4.0],
            blur: 6.0,
            spread: 2.0,
            inset: false,
        }),
        ..UiRectangle::default()
    });
    let vertices = ui_vertices(std::slice::from_ref(&shadow), 0..1, &[], (100, 100));
    // Its quad is the box moved 4 down, grown by the spread and the blur.
    assert_eq!(vertices[0].size, [36.0, 36.0]);
    assert_eq!(vertices[0].border[1], UI_SHADOW);
    // Text just below the box is under the shadow, so the shadow goes in
    // an earlier layer.
    let items = [ui_label([0.0, 24.0], [20.0, 10.0]), shadow];
    assert_eq!(ui_layers(&items), vec![0..1, 1..2]);
}

#[test]
fn a_clip_cuts_images_and_rectangles_to_its_rounded_box_on_the_gpu() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let red = Arc::new(Painted::new(8, |_, _| [255, 0, 0, 255]));
    let clip = Some(crate::UiClip {
        position: [8.0, 8.0],
        size: [48.0, 48.0],
        radius: 16.0,
        ..Default::default()
    });
    // A picture bigger than its clip, and a green bar reaching past it.
    let picture = UiDrawItem::Image(crate::UiImage {
        position: [0.0, 0.0],
        size: [64.0, 64.0],
        tint: [1.0; 4],
        border_radius: 0.0,
        path: "photo.png".to_owned(),
        texture: Some(red),
        slice: None,
        clip,
        rotation_degrees: 0.0,
    });
    let bar = UiDrawItem::Rectangle(UiRectangle {
        position: [0.0, 40.0],
        size: [64.0, 8.0],
        color: [0.0, 1.0, 0.0, 1.0],
        color_end: [0.0, 1.0, 0.0, 1.0],
        clip,
        ..UiRectangle::default()
    });
    let pixel = ui_pixels(&device, &queue, vec![picture, bar], (64, 64));
    let black = [0, 0, 0, 255];
    assert_eq!(pixel(32, 20)[..3], [255, 0, 0], "inside the clip");
    assert_eq!(pixel(4, 32), black, "left of the clip");
    assert_eq!(pixel(10, 10), black, "outside its rounded corner");
    assert_eq!(pixel(52, 52), black, "outside its bottom right corner");
    assert_eq!(pixel(32, 44)[..3], [0, 255, 0], "the bar inside");
    assert_eq!(pixel(60, 44), black, "the bar past the clip");
}

#[test]
fn a_clip_intersects_with_the_one_around_it_and_knows_its_rounded_corners() {
    let outer = crate::UiClip {
        position: [0.0, 0.0],
        size: [100.0, 100.0],
        radius: 20.0,
        ..Default::default()
    };
    let inner = crate::UiClip {
        position: [50.0, -10.0],
        size: [100.0, 40.0],
        radius: 4.0,
        ..Default::default()
    };
    let both = inner.intersect(outer);
    assert_eq!(both.position, [50.0, 0.0]);
    assert_eq!(both.size, [50.0, 30.0]);
    assert_eq!(both.radius, 4.0, "the smaller one's rounding");
    assert!(outer.contains(50.0, 50.0));
    assert!(
        !outer.contains(1.0, 1.0),
        "in the corner, outside the rounding"
    );
    assert!(outer.contains(20.0, 1.0));
    assert!(!outer.contains(101.0, 50.0));
}

#[test]
fn a_turned_rectangle_draws_turned_around_its_middle_on_the_gpu() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let white = [1.0, 1.0, 1.0, 1.0];
    let square = |rotation_degrees: f32| {
        UiDrawItem::Rectangle(UiRectangle {
            position: [30.0, 30.0],
            size: [40.0, 40.0],
            color: white,
            color_end: white,
            rotation_degrees,
            ..UiRectangle::default()
        })
    };
    let black = [0, 0, 0, 255];
    let upright = ui_pixels(&device, &queue, vec![square(0.0)], (100, 100));
    assert_ne!(upright(33, 33), black, "inside the upright corner");
    assert_eq!(upright(50, 25), black, "above the upright square");
    // Turned 45°, it's a diamond: its corner is gone and its point
    // reaches above where the square's top edge was.
    let turned = ui_pixels(&device, &queue, vec![square(45.0)], (100, 100));
    assert_eq!(turned(33, 33), black, "the corner turned away");
    assert_ne!(turned(50, 25), black, "the diamond's top point");
    assert_ne!(turned(50, 50), black, "its middle stays put");
    // Its upright bounds are what overlap is worked out from.
    let (_, bounds) = kind_and_bounds_of(&square(45.0));
    assert!(
        (bounds[1] - (50.0 - 800f32.sqrt())).abs() < 0.01,
        "{bounds:?}"
    );
}

fn kind_and_bounds_of(item: &UiDrawItem) -> (u8, [f32; 4]) {
    super::super::ui_pass::kind_and_bounds(item)
}

#[test]
fn turned_text_is_drawn_turned_from_its_sheet_on_the_gpu() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let label = |rotation_degrees: f32| {
        UiDrawItem::Text(UiText {
            position: [10.0, 42.0],
            size: [80.0, 16.0],
            text: "HHHHHHHH".to_owned(),
            font_size: 14.0,
            color: [1.0, 1.0, 1.0, 1.0],
            rotation_degrees,
            ..UiText::default()
        })
    };
    // How many lit pixels there are in a box (left, top, right, bottom).
    let lit = |pixel: &dyn Fn(u32, u32) -> [u8; 4], [left, top, right, bottom]: [u32; 4]| {
        (left..right)
            .flat_map(|x| (top..bottom).map(move |y| (x, y)))
            .filter(|&(x, y)| pixel(x, y)[0] > 128)
            .count()
    };
    let across = [12, 40, 40, 60];
    let down = [40, 12, 60, 40];
    let upright = ui_pixels(&device, &queue, vec![label(0.0)], (100, 100));
    assert!(lit(&upright, across) > 20, "upright text runs across");
    assert_eq!(lit(&upright, down), 0, "and not down");
    let turned = ui_pixels(&device, &queue, vec![label(90.0)], (100, 100));
    assert!(lit(&turned, down) > 20, "turned a quarter, it runs down");
    assert_eq!(lit(&turned, across), 0, "and not across");
    // A turned text is drawn with the pictures, so it orders as one.
    assert_eq!(kind_and_bounds_of(&label(90.0)).0, 1);
}

#[test]
fn a_turned_clip_cuts_in_its_own_frame_and_intersects_with_one_turned_the_same() {
    let clip = crate::UiClip {
        position: [0.0, 40.0],
        size: [100.0, 20.0],
        radius: 0.0,
        rotation_degrees: 90.0,
    };
    // Turned a quarter around (50, 50), the bar runs down the middle.
    assert!(clip.contains(50.0, 5.0));
    assert!(!clip.contains(5.0, 50.0));
    // The top half of that bar: 50 long, its middle at (50, 25).
    let inner = crate::UiClip {
        position: [25.0, 15.0],
        size: [50.0, 20.0],
        ..clip
    };
    let both = clip.intersect(inner);
    assert_eq!(both.rotation_degrees, 90.0);
    assert!(both.contains(50.0, 20.0), "{both:?}");
    assert!(!both.contains(50.0, 90.0), "{both:?}");
}
