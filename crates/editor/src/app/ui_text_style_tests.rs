//! The UI Designer's text and look editors: the text toolbar and its keys,
//! rich text typed on the artboard, hover and pressed states, bevels and the
//! 9-slice editor, each the same in the designer and in Play.

use axcenity_render::glam::Vec3;

use super::tests::copy_of_example;
use super::ui_designer_tests::{
    act, build_menu, click_named, designer_frame, drag_on_artboard, drop_in, frame_ends, key,
    on_artboard, open_section, press, run_designer, state, stylesheets, widget_named,
};
use super::*;

#[test]
fn the_text_toolbar_writes_bold_italic_underline_into_the_elements_own_rule() {
    let folder = copy_of_example("hello-touched", "ui-designer-text-toolbar");
    let mut app = EditorApp::new(Some(&folder));
    act(&mut app, DesignerAction::NewScreenGui);
    let gui = app.selected.unwrap();
    let title = drop_in(&mut app, Insertable::TextLabel, gui, [10.0, 10.0]);
    act(
        &mut app,
        DesignerAction::Edit(title, Edit::Name("Title".into())),
    );
    let before = state(&app);
    let style =
        |app: &EditorApp| axcenity_ui::text_style(&app.world, title, &stylesheets(app)).unwrap();
    let toggle = |app: &mut EditorApp, toggle: crate::ui_designer::Toggle| {
        let action = toggle.action(title, &style(app));
        act(app, action);
    };
    use crate::ui_designer::Toggle;
    toggle(&mut app, Toggle::Bold);
    toggle(&mut app, Toggle::Italic);
    toggle(&mut app, Toggle::Underline);

    let css = app.stylesheet_text("ui.css").unwrap().replace("\r\n", "\n");
    assert!(
        css.contains(
            "#Title {\n  font-weight: bold;\n  font-style: italic;\n  text-decoration: underline;\n}"
        ),
        "{css}"
    );
    // Drawn bold, italic and underlined in Montserrat, the same on the
    // artboard as in Play.
    let sheets = stylesheets(&app);
    let drawn = |items: &[axcenity_render::UiDrawItem]| {
        items
            .iter()
            .find_map(|item| match item {
                axcenity_render::UiDrawItem::Text(text) => Some(text.clone()),
                _ => None,
            })
            .unwrap()
    };
    let (width, height) = axcenity_ui::design_size(&app.world, gui);
    let play = drawn(&axcenity_ui::layout_ui(&app.world, width, height, &sheets).draw_items);
    let board = drawn(
        &axcenity_ui::layout_artboard(&app.world, gui, &sheets, (width, height), 1.0, [0.0, 0.0])
            .draw_items,
    );
    assert_eq!(play, board);
    assert_eq!(
        (
            play.font_family.as_str(),
            play.weight,
            play.italic,
            play.underline
        ),
        ("Montserrat", 700, true, true)
    );

    // Toggling again changes the declaration instead of adding another,
    // and strikethrough keeps the underline.
    toggle(&mut app, Toggle::Bold);
    toggle(&mut app, Toggle::Strikethrough);
    let css = app.stylesheet_text("ui.css").unwrap();
    assert_eq!(css.matches("font-weight").count(), 1, "{css}");
    assert!(css.contains("font-weight: normal"), "{css}");
    assert!(
        css.contains("text-decoration: underline line-through"),
        "{css}"
    );

    // Other controls: a value dragged over several frames, Wrap, and Text
    // Scaled (a property, not CSS).
    act(
        &mut app,
        DesignerAction::TextStyle {
            id: title,
            property: "letter-spacing",
            value: "1px".into(),
            dragging: false,
        },
    );
    for spacing in ["2px", "3px", "4px"] {
        app.designer_actions(vec![DesignerAction::TextStyle {
            id: title,
            property: "letter-spacing",
            value: spacing.into(),
            dragging: true,
        }]);
    }
    frame_ends(&mut app);
    assert_eq!(style(&app).letter_spacing, 4.0);
    act(
        &mut app,
        DesignerAction::TextStyle {
            id: title,
            property: "white-space",
            value: "nowrap".into(),
            dragging: false,
        },
    );
    act(
        &mut app,
        DesignerAction::Edit(title, Edit::Property("TextScaled", Value::Bool(true))),
    );
    let now = style(&app);
    assert!(!now.wrap && now.scaled);

    // Every edit is an undo step; the drag is one. The first one also takes
    // away ui.css.
    for _ in 0..9 {
        app.undo();
    }
    assert_eq!(state(&app), before);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn ctrl_b_i_u_toggle_the_selected_labels_text_unless_typing() {
    let folder = copy_of_example("hello-touched", "ui-designer-text-shortcuts");
    let mut app = EditorApp::new(Some(&folder));
    act(&mut app, DesignerAction::NewScreenGui);
    let gui = app.selected.unwrap();
    let title = drop_in(&mut app, Insertable::TextLabel, gui, [10.0, 10.0]);
    let sheets = stylesheets(&app);
    let scene = DesignerScene {
        world: &app.world,
        selected: Some(title),
        selection: &[],
        stylesheets: &sheets,
        assets: &app.assets,
        images: &[],
        scripts: &[],
        editing: true,
    };
    let press = |key: egui::Key| egui::RawInput {
        events: std::iter::once(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND))
            .chain(vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }])
            .collect(),
        ..Default::default()
    };
    let ctx = crate::theme::test_context();
    let mut designer = crate::ui_designer::UiDesigner::default();
    let mut actions = Vec::new();
    for key in [egui::Key::B, egui::Key::I, egui::Key::U] {
        let _ = crate::theme::run(&ctx, press(key), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                actions.extend(designer.show(ui, &scene, None));
            });
        });
    }
    let toggles: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            DesignerAction::TextStyle {
                property, value, ..
            } => Some((*property, value.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(
        toggles,
        [
            ("font-weight", "bold"),
            ("font-style", "italic"),
            ("text-decoration", "underline"),
        ]
    );

    // While a text box has focus, the keys are the text box's.
    let mut typed = Vec::new();
    let mut text = String::new();
    for key in [egui::Key::B, egui::Key::I, egui::Key::U] {
        let _ = crate::theme::run(&ctx, press(key), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let field = ui.text_edit_singleline(&mut text);
                field.request_focus();
                typed.extend(designer.show(ui, &scene, None));
            });
        });
    }
    assert!(
        !typed
            .iter()
            .any(|action| matches!(action, DesignerAction::TextStyle { .. })),
        "{typed:?}"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn selected_text_in_the_typing_box_is_wrapped_in_rich_text_tags() {
    let folder = copy_of_example("hello-touched", "ui-designer-rich-text");
    let mut app = EditorApp::new(Some(&folder));
    let menu = build_menu(&mut app);
    act(
        &mut app,
        DesignerAction::Edit(menu.title, Edit::Property("RichText", Value::Bool(true))),
    );
    let ctx = theme::test_context();
    let mut clock = 0.0;
    let size = egui::vec2(1100.0, 600.0);
    run_designer(&mut app, &ctx, &mut clock, size, vec![Vec::new(); 3]);
    let at = on_artboard(&app, menu.gui, menu.title);
    let double_click = vec![
        vec![egui::Event::PointerMoved(at)],
        vec![press(at, true)],
        vec![press(at, false)],
        vec![press(at, true)],
        vec![press(at, false)],
        Vec::new(),
    ];
    let text = |app: &EditorApp| {
        app.world
            .get(menu.title)
            .unwrap()
            .property("Text")
            .and_then(Value::as_str)
            .unwrap()
            .to_owned()
    };
    // "Main" selected: Home, then Shift+Right four times.
    let select_main: Vec<Vec<egui::Event>> =
        std::iter::once(vec![key(egui::Key::Home, egui::Modifiers::NONE)])
            .chain((0..4).map(|_| vec![key(egui::Key::ArrowRight, egui::Modifiers::SHIFT)]))
            .collect();
    let undo_before = app.history.undo_label().map(str::to_owned);
    run_designer(&mut app, &ctx, &mut clock, size, double_click.clone());
    run_designer(&mut app, &ctx, &mut clock, size, select_main.clone());
    run_designer(
        &mut app,
        &ctx,
        &mut clock,
        size,
        vec![vec![key(egui::Key::B, egui::Modifiers::COMMAND)]],
    );
    // Still typing, with the tags round the word.
    assert_eq!(text(&app), "<b>Main</b> Menu");
    run_designer(
        &mut app,
        &ctx,
        &mut clock,
        size,
        vec![
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
            Vec::new(),
        ],
    );
    assert_eq!(text(&app), "<b>Main</b> Menu");
    // It draws one bold word, and the element's own style didn't change.
    let layout = axcenity_ui::layout_ui(&app.world, 1280, 720, &stylesheets(&app));
    let drawn = layout
        .draw_items
        .iter()
        .find_map(|item| match item {
            axcenity_render::UiDrawItem::Text(text) if text.text == "Main Menu" => Some(text),
            _ => None,
        })
        .unwrap();
    assert_eq!(drawn.spans[0].text, "Main");
    assert_eq!(drawn.spans[0].weight, 700);
    assert_eq!(drawn.spans[1].weight, 400);
    // One undo step puts it back.
    assert_eq!(app.history.undo_label(), Some("Change Text of Title"));
    app.undo();
    assert_eq!(text(&app), "Main Menu");
    assert_eq!(app.history.undo_label().map(str::to_owned), undo_before);
    app.redo();

    // The bar over the box: a colour on "Menu", and typing carries on.
    run_designer(&mut app, &ctx, &mut clock, size, double_click.clone());
    run_designer(
        &mut app,
        &ctx,
        &mut clock,
        size,
        vec![
            vec![key(egui::Key::End, egui::Modifiers::NONE)],
            vec![key(egui::Key::ArrowLeft, egui::Modifiers::SHIFT)],
            vec![key(egui::Key::ArrowLeft, egui::Modifiers::SHIFT)],
            vec![key(egui::Key::ArrowLeft, egui::Modifiers::SHIFT)],
            vec![key(egui::Key::ArrowLeft, egui::Modifiers::SHIFT)],
        ],
    );
    let output = designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    let red = widget_named(
        &output,
        "Colour: <font color=\"#ef476f\">",
        ctx.pixels_per_point(),
    )
    .expect("the bar is over the box");
    assert!(red.y < at.y, "the bar is above the box");
    for events in [
        vec![egui::Event::PointerMoved(red)],
        vec![press(red, true)],
        vec![press(red, false)],
        Vec::new(),
    ] {
        designer_frame(&mut app, &ctx, &mut clock, size, events);
    }
    assert_eq!(
        text(&app),
        "<b>Main</b> <font color=\"#ef476f\">Menu</font>"
    );
    run_designer(
        &mut app,
        &ctx,
        &mut clock,
        size,
        vec![
            vec![key(egui::Key::End, egui::Modifiers::NONE)],
            vec![egui::Event::Text("!".into())],
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
            Vec::new(),
        ],
    );
    assert_eq!(
        text(&app),
        "<b>Main</b> <font color=\"#ef476f\">Menu</font>!"
    );

    // The box being typed in shows the tags' styles, with the bar over it.
    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from) {
        app.ui_designer = UiDesigner::default();
        let frames = std::iter::repeat_n(Vec::new(), 3)
            // Snapshot frames are half a second apart: both clicks in one.
            .chain([vec![egui::Event::PointerMoved(at)]])
            .chain([double_click.concat()])
            .chain(select_main)
            .chain([Vec::new()])
            .collect();
        crate::snapshot::snapshot_rendered(
            &snapshots.join("ui-designer-rich-text.png"),
            size,
            frames,
            |ui, render_state| {
                egui::CentralPanel::default()
                    .show(ui, |ui| app.ui_designer_pane(ui, Some(render_state)));
            },
        );
    }
    let _ = std::fs::remove_dir_all(&folder);
}

/// The text of the rule written exactly as `selector` in `css`, between its braces.
fn rule_text<'a>(css: &'a str, selector: &str) -> &'a str {
    let start = css
        .find(&format!("{selector} {{"))
        .unwrap_or_else(|| panic!("{selector} in:\n{css}"));
    let open = start + css[start..].find('{').unwrap() + 1;
    let close = open + css[open..].find('}').unwrap();
    &css[open..close]
}

#[test]
fn the_states_editor_writes_hover_and_pressed_rules_that_show_in_its_preview_and_in_play() {
    use crate::ui_designer::state_style_action;
    use axcenity_render::UiDrawItem;
    use axcenity_ui::{UiInput, UiInteraction, UiState};
    let folder = copy_of_example("hello-touched", "ui-designer-states");
    let mut app = EditorApp::new(Some(&folder));
    act(&mut app, DesignerAction::NewScreenGui);
    let gui = app.selected.unwrap();
    act(
        &mut app,
        DesignerAction::DesignSize {
            gui,
            width: 1280,
            height: 720,
        },
    );
    let play = drop_in(&mut app, Insertable::TextButton, gui, [100.0, 100.0]);
    act(
        &mut app,
        DesignerAction::Edit(play, Edit::Name("Play".into())),
    );
    // The Normal tab's Background.
    act(
        &mut app,
        state_style_action(
            play,
            UiState::Normal,
            "background-color",
            "#3a86ff".into(),
            false,
        ),
    );
    let before = state(&app);

    // The tabs, as clicked: Hover shows the eye on the artboard, which
    // shows the button hovered until it's clicked off.
    let ctx = theme::test_context();
    open_section(&ctx, crate::ui_designer::Section::States);
    let mut clock = 0.0;
    // Tall, so the fine-tuning sections at the end of the inspector show.
    let size = egui::vec2(1100.0, 2400.0);
    for _ in 0..3 {
        designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    }
    click_named(&mut app, &ctx, &mut clock, size, "Hover");
    let output = designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    let ppp = ctx.pixels_per_point();
    assert!(
        widget_named(
            &output,
            &format!("{} Showing Hover", theme::icons::EYE),
            ppp
        )
        .is_some(),
        "the eye shows the state on the artboard"
    );
    click_named(
        &mut app,
        &ctx,
        &mut clock,
        size,
        &format!("{} Showing Hover", theme::icons::EYE),
    );
    let output = designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    assert!(
        widget_named(
            &output,
            &format!("{} Show Hover", theme::icons::EYE_SLASH),
            ppp
        )
        .is_some()
    );

    // A darker Hover background and a 0.95 Pressed scale, from the states
    // editor's controls (no CSS typed): each a rule of its own, one step each.
    act(
        &mut app,
        state_style_action(
            play,
            UiState::Hover,
            "background-color",
            "#1d4ed8".into(),
            false,
        ),
    );
    act(
        &mut app,
        state_style_action(
            play,
            UiState::Pressed,
            "transform",
            "scale(0.95)".into(),
            false,
        ),
    );
    // Its Transition, in #Play.
    act(
        &mut app,
        state_style_action(
            play,
            UiState::Normal,
            "transition",
            "all 0.15s ease".into(),
            false,
        ),
    );
    let css = app.stylesheet_text("ui.css").unwrap().replace("\r\n", "\n");
    assert_eq!(
        rule_text(&css, "#Play:hover").trim(),
        "background-color: #1d4ed8;"
    );
    assert_eq!(
        rule_text(&css, "#Play:active").trim(),
        "transform: scale(0.95);"
    );
    assert!(
        rule_text(&css, "#Play").contains("transition: all 0.15s ease"),
        "{css}"
    );
    // Changing the Hover colour again changes it in place.
    act(
        &mut app,
        state_style_action(
            play,
            UiState::Hover,
            "background-color",
            "#1e40af".into(),
            false,
        ),
    );
    let css = app.stylesheet_text("ui.css").unwrap().replace("\r\n", "\n");
    assert_eq!(
        rule_text(&css, "#Play:hover").trim(),
        "background-color: #1e40af;"
    );

    // The states editor's preview on the artboard.
    let sheets = stylesheets(&app);
    let background = |items: &[UiDrawItem]| {
        items
            .iter()
            .find_map(|item| match item {
                UiDrawItem::Rectangle(rectangle) => Some(*rectangle),
                _ => None,
            })
            .unwrap()
    };
    let shown = |state| {
        axcenity_ui::layout_artboard_in_state(
            &app.world,
            gui,
            &sheets,
            (1280, 720),
            1.0,
            [0.0, 0.0],
            Some((play, state)),
        )
        .draw_items
    };
    let hover_blue = [
        0x1e as f32 / 255.0,
        0x40 as f32 / 255.0,
        0xaf as f32 / 255.0,
    ];
    assert_eq!(background(&shown(UiState::Hover)).color[..3], hover_blue);
    let pressed = background(&shown(UiState::Pressed));
    assert_eq!(pressed.color[..3], hover_blue);
    assert!((pressed.size[0] - 160.0 * 0.95).abs() < 1e-3);

    // The designer with the Hover tab picked and its eye on, to look at.
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from) {
        act(&mut app, DesignerAction::Select(Some(play)));
        app.ui_designer = UiDesigner::default();
        app.ui_designer.show_in_state(UiState::Hover, true);
        crate::snapshot::snapshot_rendered(
            &folder.join("ui-designer-states.png"),
            egui::vec2(1100.0, 1000.0),
            vec![Vec::new(); 3],
            |ui, render_state| {
                egui::CentralPanel::default()
                    .show(ui, |ui| app.ui_designer_pane(ui, Some(render_state)));
            },
        );
    }

    // In Play, the pointer over it and then holding it shows the same,
    // after the transition.
    app.play();
    let session = app.play.as_ref().expect("playing");
    let mut ui = UiInteraction::default();
    let over = UiInput {
        pointer: Some([180.0, 124.0]),
        pressed: None,
        focused: None,
        on_top: None,
    };
    let held = UiInput {
        pressed: Some(play),
        ..over
    };
    let mut frame = |input, seconds| {
        background(
            &ui.layout(session.world(), (1280, 720), &sheets, input, None, seconds)
                .draw_items,
        )
    };
    frame(UiInput::default(), 0.0);
    frame(over, 0.0);
    assert_eq!(frame(over, 0.3).color[..3], hover_blue);
    frame(held, 0.0);
    assert!((frame(held, 0.3).size[0] - 152.0).abs() < 1e-3);
    app.stop();

    // Every edit undoes, as one step each.
    for _ in 0..4 {
        app.undo();
    }
    assert_eq!(state(&app), before);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn mouse_enter_leave_and_a_disabled_button_reach_the_game_in_play() {
    use axcenity_ui::UiEvent;
    let folder = copy_of_example("hello-buttons", "ui-designer-hover-play");
    let mut app = EditorApp::new(Some(&folder));
    app.play();
    let world = app.play.as_ref().expect("playing").world();
    let menu = world.find_first_child(world.workspace(), "Menu").unwrap();
    let play = world.find_first_child(menu, "Play").unwrap();
    let locked = world.find_first_child(menu, "Locked").unwrap();
    let status = world.find_first_child(menu, "Status").unwrap();
    let status_text = |app: &EditorApp| {
        app.play
            .as_ref()
            .unwrap()
            .world()
            .get(status)
            .and_then(|instance| instance.property("Text"))
            .and_then(Value::as_str)
            .unwrap()
            .to_owned()
    };

    // What the viewport hands on when the pointer goes onto Play.
    app.pending_ui_events = vec![
        UiEvent::MouseEnter(play),
        UiEvent::PlaySound("assets/audio/click.wav".into()),
    ];
    app.play_frame(1.0 / 60.0, &[], None, None);
    assert_eq!(status_text(&app), "Ready?");
    assert!(
        super::tests::printed(&app).contains(&"The pointer is on Play (blocks)"),
        "{:?}",
        super::tests::printed(&app)
    );
    // A click on Locked never reaches it: Interactable is off.
    app.play_frame(1.0 / 60.0, &[], Some(locked), None);
    assert_eq!(status_text(&app), "Ready?");
    app.pending_ui_events = vec![UiEvent::MouseLeave(play)];
    app.play_frame(1.0 / 60.0, &[], None, None);
    assert_eq!(status_text(&app), "Point at Play");
    assert!(super::tests::printed(&app).contains(&"The pointer left Play (blocks)"));
    app.stop();
    let _ = std::fs::remove_dir_all(&folder);
}

/// `items` drawn on a `width` × `height` screen, as the brightness (0 to
/// 765) of each pixel, or `None` on a machine without a GPU.
fn brightness_of(
    items: Vec<axcenity_render::UiDrawItem>,
    (width, height): (u32, u32),
) -> Option<impl Fn(f32, f32) -> u32> {
    use axcenity_render::wgpu;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).ok()?;
    let mut list = axcenity_scene::build_draw_list(
        &World::new(),
        &axcenity_assets::Assets::default(),
        axcenity_render::Camera {
            position: Vec3::ZERO,
            look_at: Vec3::NEG_Z,
            fov_y_degrees: 70.0,
        },
    );
    list.ui = items;
    let mut renderer = axcenity_render::TextureRenderer::new(&device, &queue, width, height);
    renderer.render(&list).ok()?;
    let pixels = renderer.read_pixels();
    Some(move |x: f32, y: f32| {
        let at = ((y as u32 * width + x as u32) * 4) as usize;
        pixels[at..at + 3].iter().map(|&c| u32::from(c)).sum()
    })
}

#[test]
fn a_raised_bevel_lights_the_top_left_and_flips_while_held_in_the_designer_and_in_play() {
    use crate::ui_designer::bevel::{Bevel, find_bevel};
    use crate::ui_designer::state_style_action;
    use axcenity_render::UiDrawItem;
    use axcenity_ui::{UiInput, UiInteraction, UiState};
    let folder = copy_of_example("hello-touched", "ui-designer-bevel");
    let mut app = EditorApp::new(Some(&folder));
    act(&mut app, DesignerAction::NewScreenGui);
    let gui = app.selected.unwrap();
    act(
        &mut app,
        DesignerAction::DesignSize {
            gui,
            width: 1280,
            height: 720,
        },
    );
    let play = drop_in(&mut app, Insertable::TextButton, gui, [100.0, 100.0]);
    act(
        &mut app,
        DesignerAction::Edit(play, Edit::Name("Play".into())),
    );
    act(
        &mut app,
        state_style_action(
            play,
            UiState::Normal,
            "background-color",
            "#808080".into(),
            false,
        ),
    );
    let before = state(&app);

    // Raised, clicked in the inspector's Bevel section.
    let ctx = theme::test_context();
    open_section(&ctx, crate::ui_designer::Section::Bevel);
    let mut clock = 0.0;
    // Tall, so the fine-tuning sections at the end of the inspector show.
    let size = egui::vec2(1100.0, 2400.0);
    for _ in 0..3 {
        designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    }
    click_named(&mut app, &ctx, &mut clock, size, "Raised");
    let css = app.stylesheet_text("ui.css").unwrap().replace("\r\n", "\n");
    let shadows_in = |selector: &str| {
        let rule = rule_text(&css, selector);
        let start = rule.find("box-shadow:").unwrap_or_else(|| panic!("{css}")) + 11;
        rule[start..start + rule[start..].find(';').unwrap()]
            .trim()
            .to_owned()
    };
    let raised = Bevel::default();
    assert_eq!(find_bevel(&shadows_in("#Play")), Some((raised, 0)), "{css}");
    assert_eq!(
        find_bevel(&shadows_in("#Play:active")),
        Some((raised.flipped(), 0)),
        "{css}"
    );

    // Lit at the top left and shaded at the bottom right, then the other
    // way round while held: on the artboard, and in Play.
    let sheets = stylesheets(&app);
    let shown = |state| {
        axcenity_ui::layout_artboard_in_state(
            &app.world,
            gui,
            &sheets,
            (1280, 720),
            1.0,
            [0.0, 0.0],
            Some((play, state)),
        )
    };
    let rect = shown(UiState::Normal).element(play).unwrap().rect;
    // The light inset shadow is moved down and right when raised, so it
    // lies along the top and left edges, and up and left when sunken.
    let light_shadow = |items: &[UiDrawItem]| {
        items
            .iter()
            .find_map(|item| match item {
                UiDrawItem::Rectangle(r)
                    if r.shadow.is_some_and(|s| s.inset) && r.color[..3] == [1.0; 3] =>
                {
                    Some(*r)
                }
                _ => None,
            })
            .unwrap()
    };
    let check = |items: Vec<UiDrawItem>, raised: bool, place: &str| {
        let offset = light_shadow(&items).shadow.unwrap().offset;
        assert_eq!(offset[0] > 0.0 && offset[1] > 0.0, raised, "{place}");
        let Some(brightness) = brightness_of(items, (1280, 720)) else {
            eprintln!("no GPU: the pixels weren't checked");
            return;
        };
        let middle = [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
        let top = brightness(middle[0], rect.y + 1.0);
        let left = brightness(rect.x + 1.0, middle[1]);
        let bottom = brightness(middle[0], rect.y + rect.height - 2.0);
        let right = brightness(rect.x + rect.width - 2.0, middle[1]);
        if raised {
            assert!(
                top > bottom && left > right,
                "{place}: {top} {left} {bottom} {right}"
            );
        } else {
            assert!(
                top < bottom && left < right,
                "{place}: {top} {left} {bottom} {right}"
            );
        }
    };
    check(shown(UiState::Normal).draw_items, true, "designer");
    check(
        shown(UiState::Pressed).draw_items,
        false,
        "designer, pressed",
    );

    app.play();
    let session = app.play.as_ref().expect("playing");
    let mut ui = UiInteraction::default();
    let over = UiInput {
        pointer: Some([rect.x + 20.0, rect.y + 10.0]),
        ..UiInput::default()
    };
    let held = UiInput {
        pressed: Some(play),
        ..over
    };
    let mut frame = |input, seconds| {
        ui.layout(session.world(), (1280, 720), &sheets, input, None, seconds)
            .draw_items
    };
    frame(UiInput::default(), 0.0);
    check(frame(over, 0.1), true, "Play");
    check(frame(held, 0.1), false, "Play, held");
    app.stop();

    // Both rules were one undo step.
    app.undo();
    assert_eq!(state(&app), before);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn dragging_the_slice_editors_lines_changes_the_border_image_css() {
    let folder = copy_of_example("hello-touched", "ui-designer-slice");
    // A 40 × 40 picture to cut.
    let picture = folder.join("assets/ui/frame.png");
    std::fs::create_dir_all(picture.parent().unwrap()).unwrap();
    image::RgbaImage::from_fn(40, 40, |x, y| {
        image::Rgba([(x * 6) as u8, (y * 6) as u8, 128, 255])
    })
    .save(&picture)
    .unwrap();
    let mut app = EditorApp::new(Some(&folder));
    act(&mut app, DesignerAction::NewScreenGui);
    let gui = app.selected.unwrap();
    let panel = drop_in(&mut app, Insertable::Frame, gui, [100.0, 100.0]);
    act(
        &mut app,
        DesignerAction::Edit(panel, Edit::Name("Panel".into())),
    );
    // What the Image box writes for a picture.
    act(
        &mut app,
        DesignerAction::TextStyle {
            id: panel,
            property: "border-image",
            value: "url(\"assets/ui/frame.png\") 10 fill".into(),
            dragging: false,
        },
    );
    let before = state(&app);
    let border_image = |app: &EditorApp| {
        axcenity_ui::style_value_in_state(
            &app.world,
            panel,
            &stylesheets(app),
            axcenity_ui::UiState::Normal,
            "border-image",
        )
        .and_then(|value| axcenity_ui::BorderImage::parse(&value))
        .unwrap()
    };

    let ctx = theme::test_context();
    open_section(&ctx, crate::ui_designer::Section::Slice);
    let mut clock = 0.0;
    // Tall, so the fine-tuning sections at the end of the inspector show.
    let size = egui::vec2(1100.0, 2400.0);
    for _ in 0..3 {
        designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
    }
    // Each line dragged 10 points inward: 40 pixels drawn as 200 points
    // wide at most, so 5 points a pixel and 2 pixels a drag.
    for side in 0..4 {
        let output = designer_frame(&mut app, &ctx, &mut clock, size, Vec::new());
        let update = output.platform_output.accesskit_update.as_ref().unwrap();
        let ppp = ctx.pixels_per_point();
        let (_, node) = update
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Slice preview"))
            .expect("the Slice editor shows the picture");
        let bounds = node.bounds().unwrap();
        let preview = egui::Rect::from_min_max(
            egui::pos2(bounds.x0 as f32 / ppp, bounds.y0 as f32 / ppp),
            egui::pos2(bounds.x1 as f32 / ppp, bounds.y1 as f32 / ppp),
        );
        let points = preview.width() / 40.0;
        let slices = border_image(&app).slices;
        let middle = preview.center();
        let (from, inward) = match side {
            0 => (
                egui::pos2(middle.x, preview.top() + slices[0] * points),
                egui::vec2(0.0, 1.0),
            ),
            1 => (
                egui::pos2(preview.right() - slices[1] * points, middle.y),
                egui::vec2(-1.0, 0.0),
            ),
            2 => (
                egui::pos2(middle.x, preview.bottom() - slices[2] * points),
                egui::vec2(0.0, -1.0),
            ),
            _ => (
                egui::pos2(preview.left() + slices[3] * points, middle.y),
                egui::vec2(1.0, 0.0),
            ),
        };
        for events in drag_on_artboard(from, from + inward * points * 2.0) {
            designer_frame(&mut app, &ctx, &mut clock, size, events);
        }
        let mut expected = slices;
        expected[side] += 2.0;
        assert_eq!(border_image(&app).slices, expected, "dragging line {side}");
    }
    assert_eq!(border_image(&app).slices, [12.0; 4]);
    // The inspector's Bevel and Slice sections, and the frame, to look at.
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from) {
        let (normal, pressed) = (
            crate::ui_designer::bevel::with_bevel(
                None,
                Some(crate::ui_designer::bevel::Bevel::default()),
            ),
            None,
        );
        act(
            &mut app,
            DesignerAction::Bevel {
                id: panel,
                normal,
                pressed,
                dragging: false,
            },
        );
        app.ui_designer = UiDesigner::default();
        crate::snapshot::snapshot_rendered(
            &folder.join("ui-designer-slice.png"),
            egui::vec2(1100.0, 1300.0),
            vec![Vec::new(); 3],
            |ui, render_state| {
                egui::CentralPanel::default()
                    .show(ui, |ui| app.ui_designer_pane(ui, Some(render_state)));
            },
        );
        app.undo();
    }
    let css = app.stylesheet_text("ui.css").unwrap();
    assert!(
        css.contains("border-image: url(\"assets/ui/frame.png\") 12 fill"),
        "{css}"
    );
    // Each drag is one undo step.
    for _ in 0..4 {
        app.undo();
    }
    assert_eq!(state(&app), before);
    let _ = std::fs::remove_dir_all(&folder);
}
