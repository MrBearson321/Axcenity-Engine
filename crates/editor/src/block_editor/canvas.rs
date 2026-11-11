use super::*;

/// Above a loose stack: why it does nothing.
pub(super) const LOOSE_NOTE: &str = "Not under an event block, so it doesn't run";

/// The canvas with the script's stacks. Returns where dragged blocks can go.
pub(super) fn show_canvas(
    ui: &mut egui::Ui,
    rect: Rect,
    script: &mut BlockScript,
    state: &mut State,
    context: &BlockContext,
    variables: &[String],
) -> (StackSpots, SlotSpots) {
    let dark = ui.visuals().dark_mode;
    let background = if dark {
        Color32::from_rgb(30, 33, 41)
    } else {
        Color32::from_rgb(249, 249, 251)
    };
    let mut canvas = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    canvas.set_clip_rect(rect);
    let painter = canvas.painter_at(rect);
    painter.rect_filled(rect, 0, background);

    // Dragging the background scrolls; so do the wheel and trackpad, and
    // Ctrl + wheel (or pinching) zooms around the pointer.
    let back = canvas.interact(
        rect,
        egui::Id::new(("block canvas", context.key)),
        Sense::click_and_drag(),
    );
    if back.dragged() {
        state.pan += back.drag_delta();
    }
    if canvas.rect_contains_pointer(rect) && state.drag.is_none() {
        let (scroll, zoom, pointer) =
            canvas.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.pointer.hover_pos()));
        state.pan += scroll;
        if zoom != 1.0
            && let Some(pointer) = pointer
        {
            let old = state.zoom;
            state.zoom = (old * zoom).clamp(0.5, 2.0);
            let from_origin = pointer - rect.min - state.pan;
            state.pan = pointer - rect.min - from_origin * (state.zoom / old);
        }
    }
    let z = state.zoom;

    // A dot grid, which moves with the blocks.
    let spacing = 24.0 * z;
    let dot = if dark {
        Color32::from_white_alpha(18)
    } else {
        Color32::from_black_alpha(22)
    };
    let start = rect.min
        + vec2(
            state.pan.x.rem_euclid(spacing),
            state.pan.y.rem_euclid(spacing),
        );
    let mut y = start.y;
    while y < rect.bottom() {
        let mut x = start.x;
        while x < rect.right() {
            painter.circle_filled(pos2(x, y), 1.2, dot);
            x += spacing;
        }
        y += spacing;
    }

    if script.blocks.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Drag a yellow Events block, such as \"when game starts\", here to begin.",
            FontId::proportional(14.0),
            ui.visuals().weak_text_color(),
        );
    }

    let origin = rect.min + state.pan;
    let mut blocks = BlockPainter::new(
        painter.clone(),
        z,
        true,
        background,
        egui::Id::new(("blocks", context.key)),
        context,
        variables,
    );
    let mut auto_y = 20.0;
    let mut grabbed_script = None;
    let count = script.blocks.len();
    for index in 0..count {
        let block = &mut script.blocks[index];
        let position = block.position.unwrap_or([20.0, auto_y]);
        let at = origin + vec2(position[0], position[1]) * z;
        if matches!(block.kind, BlockKind::Unattached { .. }) {
            blocks.stack_spots.push((at, StackSpot::Above(index)));
            // Loose blocks look like any others, so say they don't run (as
            // the Graph view does for nodes wired to no event).
            painter.text(
                at + vec2(2.0, -4.0) * z,
                egui::Align2::LEFT_BOTTOM,
                LOOSE_NOTE,
                FontId::proportional(11.0 * z),
                ui.visuals().weak_text_color(),
            );
        }
        let drawn = blocks.script(&mut canvas, block, at);
        if let Some(grab) = drawn.grabbed {
            grabbed_script = Some((index, grab));
        }
        auto_y = position[1] + drawn.height / z + STACK_GAP;
    }
    blocks.finish();

    // What was done to blocks this frame.
    for action in std::mem::take(&mut blocks.actions) {
        apply_action(script, action);
    }
    if let Some((index, grab)) = grabbed_script {
        let block = script.blocks.remove(index);
        blocks.started = Some(Drag {
            item: Dragged::Stack(vec![block]),
            grab,
            size: Vec2::ZERO,
        });
    }
    if let Some(drag) = blocks.started.take() {
        state.drag = Some(drag);
    }
    // A loose stack that's been emptied is gone.
    script
        .blocks
        .retain(|block| !matches!(&block.kind, BlockKind::Unattached { body } if body.is_empty()));
    // Positions are only stored once a stack is moved, so stacks that
    // haven't been don't change the file.
    (blocks.stack_spots, blocks.slot_spots)
}

pub(super) fn apply_action(script: &mut BlockScript, action: Action) {
    match action {
        Action::Delete(id) => {
            if let Some((stack, index)) = script.stack_of_mut(id) {
                stack.remove(index);
            }
        }
        Action::AddInput(id) => {
            if let Some(list) = script
                .find_mut(id)
                .and_then(|block| block.kind.input_list_mut())
            {
                list.push(Expression::Nil);
            }
        }
        Action::RemoveInput(id) => {
            if let Some(list) = script
                .find_mut(id)
                .and_then(|block| block.kind.input_list_mut())
            {
                list.pop();
            }
        }
        Action::Duplicate(id) => {
            if let Some(index) = script.blocks.iter().position(|block| block.id == id) {
                let mut copy = script.blocks[index].duplicate();
                let [x, y] = copy.position.unwrap_or([20.0, 20.0]);
                copy.position = Some([x + 30.0, y + 30.0]);
                script.blocks.push(copy);
            } else if let Some((stack, index)) = script.stack_of_mut(id) {
                let copy = stack[index].duplicate();
                stack.insert(index + 1, copy);
            }
        }
    }
}

/// Draws what's being dragged under the pointer, shows where it would go,
/// and drops it when the mouse button is let go.
#[allow(clippy::too_many_arguments)]
pub(super) fn finish_drag(
    ui: &mut egui::Ui,
    state: &mut State,
    script: &mut BlockScript,
    context: &BlockContext,
    variables: &[String],
    canvas: Rect,
    palette: Rect,
    stack_spots: &[(Pos2, StackSpot)],
    slot_spots: &[(Rect, SlotAddress, SlotKind)],
) {
    let Some(mut drag) = state.drag.take() else {
        return;
    };
    let (pointer, down) = ui.input(|i| (i.pointer.latest_pos(), i.pointer.primary_down()));
    let Some(pointer) = pointer else {
        state.drag = Some(drag);
        return;
    };
    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    let z = state.zoom;
    let top_left = pointer - drag.grab;
    let over_palette = palette.contains(pointer);

    // Where it would go.
    let mut stack_target = None;
    let mut slot_target = None;
    match &drag.item {
        Dragged::Stack(blocks) => {
            let event = blocks.first().is_some_and(|block| block.kind.is_event());
            let ends_in_cap = blocks.last().is_some_and(|block| block.kind.is_cap());
            if !event && !over_palette {
                let mut best = SNAP * z;
                for (at, spot) in stack_spots {
                    let fits = match spot {
                        StackSpot::After { last, .. } => !ends_in_cap || *last,
                        StackSpot::Mouth { empty, .. } => !ends_in_cap || *empty,
                        StackSpot::Above(_) => !ends_in_cap,
                    };
                    let place = match spot {
                        StackSpot::Above(_) => *at - vec2(0.0, drag.size.y - TAB_DEPTH * z),
                        _ => *at,
                    };
                    let distance = (top_left - place).length();
                    if fits && distance < best {
                        best = distance;
                        stack_target = Some((place, spot.clone()));
                    }
                }
            }
        }
        Dragged::Reporter(expression) => {
            let shape = expression.shape();
            slot_target = slot_spots
                .iter()
                .filter(|(rect, _, kind)| rect.expand(4.0).contains(pointer) && kind.accepts(shape))
                .min_by(|a, b| a.0.area().total_cmp(&b.0.area()))
                .cloned();
        }
    }

    let layer = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new("dragged blocks"),
    ));
    // Where the blocks would snap: a faint accent-tinted outline.
    let accent = theme::palette(ui.ctx()).accent;
    if let Some((place, _)) = &stack_target {
        let shadow = Rect::from_min_size(
            *place,
            vec2(
                drag.size.x.max(MIN_WIDTH * z),
                drag.size.y.max(ROW_HEIGHT * z),
            ),
        );
        layer.add(egui::epaint::RectShape::new(
            shadow,
            radius(CORNER * z),
            accent.gamma_multiply(0.22),
            Stroke::new(1.5_f32, accent.gamma_multiply(0.8)),
            egui::StrokeKind::Inside,
        ));
    }
    if let Some((rect, _, _)) = &slot_target {
        layer.rect_stroke(
            rect.expand(2.0),
            radius(FIELD_CORNER * z + 2.0),
            Stroke::new(2.5_f32, Color32::WHITE),
            egui::StrokeKind::Outside,
        );
    }
    if over_palette {
        layer.rect_filled(palette, 0, Color32::from_black_alpha(60));
        layer.text(
            palette.center(),
            egui::Align2::CENTER_CENTER,
            theme::with_icon(icons::TRASH, "Drop to delete"),
            FontId::proportional(16.0),
            Color32::WHITE,
        );
    }

    // The dragged blocks, slightly see-through, under the pointer.
    let mut preview = BlockPainter::new(
        layer.clone(),
        z,
        false,
        Color32::TRANSPARENT,
        egui::Id::new("drag preview"),
        context,
        variables,
    );
    match &drag.item {
        Dragged::Stack(blocks) => {
            let mut blocks = blocks.clone();
            let mut height = 0.0;
            let mut width: f32 = 0.0;
            if blocks.first().is_some_and(|block| block.kind.is_event()) {
                for block in &mut blocks {
                    let drawn = preview.script(ui, block, top_left + vec2(0.0, height));
                    height += drawn.height;
                    width = width.max(drawn.width);
                }
            } else {
                height = preview.stack(ui, &mut blocks, top_left, true);
                width = MIN_WIDTH * z;
            }
            drag.size = vec2(width, height + TAB_DEPTH * z);
        }
        Dragged::Reporter(expression) => {
            drag.size = preview.reporter(
                ui,
                &mut expression.clone(),
                top_left,
                egui::Id::new("drag preview"),
                None,
            );
        }
    }
    preview.finish();
    ui.ctx().request_repaint();

    if down {
        state.drag = Some(drag);
        return;
    }
    // Let go: drop it.
    if over_palette {
        return;
    }
    match drag.item {
        Dragged::Stack(blocks) => {
            if let Some((_, spot)) = stack_target {
                drop_stack(script, spot, blocks, drag.size.y / z);
            } else if canvas.contains(pointer) || !blocks.is_empty() {
                let at = (top_left - canvas.min - state.pan) / z;
                let position = Some([at.x.max(0.0), at.y.max(0.0)]);
                let block = if blocks.len() == 1 && blocks[0].kind.is_event() {
                    let mut block = blocks.into_iter().next().expect("one block");
                    block.position = position;
                    block
                } else {
                    let mut block = Block::new(BlockKind::Unattached { body: blocks });
                    block.position = position;
                    block
                };
                script.blocks.push(block);
            }
        }
        Dragged::Reporter(expression) => {
            if let Some((_, address, _)) = slot_target
                && let Some(slot) = address.resolve(script)
            {
                *slot = expression;
            }
            // Dropped anywhere else, a reporter is deleted.
        }
    }
}

pub(super) fn drop_stack(
    script: &mut BlockScript,
    spot: StackSpot,
    blocks: Vec<Block>,
    height: f32,
) {
    match spot {
        StackSpot::After { block, .. } => {
            if let Some((stack, index)) = script.stack_of_mut(block) {
                stack.splice(index + 1..index + 1, blocks);
            }
        }
        StackSpot::Mouth { block, mouth, .. } => {
            if let Some(found) = script.find_mut(block)
                && let Some(body) = found.kind.bodies_mut().into_iter().nth(mouth)
            {
                body.splice(0..0, blocks);
            }
        }
        StackSpot::Above(index) => {
            if let Some(top) = script.blocks.get_mut(index)
                && let BlockKind::Unattached { body } = &mut top.kind
            {
                body.splice(0..0, blocks);
                // The stack grows upwards, so what was there stays put.
                let [x, y] = top.position.unwrap_or([20.0, 20.0]);
                top.position = Some([x, (y - height + TAB_DEPTH).max(0.0)]);
            }
        }
    }
}

pub(super) fn show_lua(ui: &mut egui::Ui, rect: Rect, script: &BlockScript) {
    ui.painter()
        .rect_filled(rect, 0, ui.visuals().extreme_bg_color);
    ui.painter().vline(
        rect.left() + 0.5,
        rect.y_range(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    let mut panel = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(12.0)));
    panel.label(
        egui::RichText::new(theme::with_icon(icons::CODE, "Lua"))
            .strong()
            .family(theme::heading_family()),
    );
    panel.weak("What these blocks become. It runs exactly like a typed script.");
    panel.separator();
    match translate(script) {
        Ok(translation) => {
            egui::ScrollArea::both()
                .id_salt("block lua")
                .auto_shrink(false)
                .show(&mut panel, |ui| {
                    let source = if translation.source.is_empty() {
                        "-- Nothing runs yet: add an Events block.".to_owned()
                    } else {
                        translation.source
                    };
                    let job =
                        lua_syntax::layout_job(&source, FontId::monospace(12.0), ui.visuals());
                    ui.add(egui::Label::new(job).selectable(true).extend());
                });
        }
        Err(err) => {
            panel.colored_label(panel.visuals().error_fg_color, err.to_string());
        }
    }
}
