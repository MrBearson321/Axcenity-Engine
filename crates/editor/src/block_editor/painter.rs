use super::*;

/// The rounded tab under a block, or the notch cut into the top of the
/// first block of a loose stack. Drawn after the blocks, so a tab (and its
/// shadow) covers the top of the block it fits onto.
pub(super) struct Tab {
    pub(super) at: Pos2,
    pub(super) fill: Color32,
    pub(super) edge: Color32,
}

/// Draws blocks with one painter, and collects what was done to them.
pub(super) struct BlockPainter<'a> {
    pub(super) painter: egui::Painter,
    pub(super) z: f32,
    /// False for the palette and the dragged blocks: nothing reacts.
    pub(super) interactive: bool,
    pub(super) objects: &'a [ObjectInfo],
    pub(super) variables: &'a [String],
    pub(super) highlighted: Option<u64>,
    /// The background, which a loose stack's notch shows.
    pub(super) background: Color32,
    pub(super) id: egui::Id,
    pub(super) tabs: Vec<Tab>,
    pub(super) notches: Vec<Tab>,
    pub(super) stack_spots: StackSpots,
    pub(super) slot_spots: SlotSpots,
    /// A drag that started this frame.
    pub(super) started: Option<Drag>,
    pub(super) actions: Vec<Action>,
}

/// How a block was drawn.
pub(super) struct Drawn {
    pub(super) height: f32,
    pub(super) width: f32,
    /// The block was grabbed this frame, from this offset.
    pub(super) grabbed: Option<Vec2>,
}

impl<'a> BlockPainter<'a> {
    pub(super) fn new(
        painter: egui::Painter,
        z: f32,
        interactive: bool,
        background: Color32,
        id: egui::Id,
        context: &'a BlockContext<'a>,
        variables: &'a [String],
    ) -> Self {
        Self {
            painter,
            z,
            interactive,
            objects: context.objects,
            variables,
            highlighted: context.highlighted,
            background,
            id,
            tabs: Vec::new(),
            notches: Vec::new(),
            stack_spots: Vec::new(),
            slot_spots: Vec::new(),
            started: None,
            actions: Vec::new(),
        }
    }

    pub(super) fn s(&self, value: f32) -> f32 {
        value * self.z
    }

    /// Labels and values: Inter Medium, which holds up on coloured fills.
    pub(super) fn font(&self) -> FontId {
        FontId::new(FONT_SIZE * self.z, theme::medium_family())
    }

    pub(super) fn code_galley(&self, ui: &egui::Ui, code: &str) -> std::sync::Arc<egui::Galley> {
        ui.fonts_mut(|fonts| {
            fonts.layout_no_wrap(
                code.to_owned(),
                FontId::monospace(FONT_SIZE * self.z * 0.95),
                Color32::WHITE,
            )
        })
    }

    pub(super) fn text_width(&self, ui: &egui::Ui, text: &str) -> f32 {
        ui.fonts_mut(|fonts| {
            fonts
                .layout_no_wrap(text.to_owned(), self.font(), Color32::WHITE)
                .size()
                .x
        })
    }

    // ---- Sizes ------------------------------------------------------------

    pub(super) fn dropdown_size(&self, ui: &egui::Ui, text: &str) -> Vec2 {
        vec2(
            self.text_width(ui, text) + self.s(28.0),
            self.s(INPUT_HEIGHT),
        )
    }

    pub(super) fn literal_size(
        &self,
        ui: &egui::Ui,
        expression: &Expression,
        kind: SlotKind,
    ) -> Vec2 {
        let height = self.s(INPUT_HEIGHT);
        match expression {
            Expression::Number { value } => vec2(
                (self.text_width(ui, &format_number(*value)) + self.s(16.0)).max(self.s(30.0)),
                height,
            ),
            Expression::Text { value } => vec2(
                (self.text_width(ui, value) + self.s(14.0)).max(self.s(36.0)),
                height,
            ),
            Expression::Boolean { .. } if kind == SlotKind::Condition => vec2(self.s(46.0), height),
            Expression::Boolean { value } => self.dropdown_size(ui, truth_name(*value)),
            Expression::Vector3 { x, y, z } => {
                let width: f32 = [x, y, z]
                    .iter()
                    .map(|value| self.axis_width(ui, **value))
                    .sum();
                vec2(width + self.s(6.0), height)
            }
            Expression::Color3 { .. } => vec2(self.s(40.0), height),
            Expression::Name { name } => self.typed_size(ui, name),
            Expression::Nil => vec2(self.text_width(ui, "nil") + self.s(14.0), height),
            Expression::Vararg => vec2(self.text_width(ui, "...") + self.s(14.0), height),
            Expression::Closure { params, .. } => self.closure_size(ui, params),
            _ => vec2(self.s(30.0), height),
        }
    }

    /// The size of a box to type a name in.
    pub(super) fn typed_size(&self, ui: &egui::Ui, text: &str) -> Vec2 {
        vec2(
            (self.text_width(ui, text) + self.s(14.0)).max(self.s(36.0)),
            self.s(INPUT_HEIGHT),
        )
    }

    fn closure_size(&self, ui: &egui::Ui, params: &str) -> Vec2 {
        vec2(
            self.text_width(ui, &closure_label(params)) + self.s(14.0),
            self.s(INPUT_HEIGHT),
        )
    }

    pub(super) fn axis_width(&self, ui: &egui::Ui, value: f64) -> f32 {
        (self.text_width(ui, &format_number(value)) + self.s(12.0)).max(self.s(28.0))
    }

    pub(super) fn part_size(&self, ui: &egui::Ui, part: &mut Part) -> Vec2 {
        match part {
            Part::Label(text) => vec2(self.text_width(ui, text), self.s(16.0)),
            Part::Icon(_) => vec2(self.s(18.0), self.s(18.0)),
            Part::Slot(expression, kind) => {
                if expression.is_literal() {
                    self.literal_size(ui, expression, *kind)
                } else {
                    self.reporter_size(ui, expression)
                }
            }
            Part::Object(text, filter) => self.dropdown_size(ui, shown_path(text, *filter)),
            Part::Property(text, _) | Part::Key(text) => self.dropdown_size(ui, text),
            Part::Variable(name) => self.dropdown_size(ui, name),
            Part::Name(name, _, _) => self.dropdown_size(ui, name),
            Part::Math(op) => self.dropdown_size(ui, op.symbol()),
            Part::Compare(op) => self.dropdown_size(ui, op.symbol()),
            Part::Logic(op) => self.dropdown_size(ui, op.lua()),
            Part::Function(function) => self.dropdown_size(ui, function.name()),
            Part::Choice(name, _) => self.dropdown_size(ui, name),
            Part::Code(code) => self.code_galley(ui, code).size() + vec2(self.s(12.0), self.s(8.0)),
            Part::Text(text) | Part::Params(text) => self.typed_size(ui, text),
        }
    }

    pub(super) fn parts_size(&self, ui: &egui::Ui, parts: &mut [Part]) -> Vec2 {
        let mut size = Vec2::ZERO;
        for (index, part) in parts.iter_mut().enumerate() {
            let part = self.part_size(ui, part);
            size.x += part.x + if index > 0 { self.s(GAP) } else { 0.0 };
            size.y = size.y.max(part.y);
        }
        size
    }

    pub(super) fn row_size(&self, ui: &egui::Ui, parts: &mut [Part]) -> Vec2 {
        let content = self.parts_size(ui, parts);
        vec2(
            content.x + 2.0 * self.s(PAD),
            (content.y + self.s(12.0)).max(self.s(ROW_HEIGHT)),
        )
    }

    pub(super) fn reporter_size(&self, ui: &egui::Ui, expression: &mut Expression) -> Vec2 {
        let boolean = expression.shape() == BlockShape::Boolean;
        let mut parts = reporter_parts(expression, self.objects);
        let content = self.parts_size(ui, &mut parts);
        let height = (content.y + self.s(8.0)).max(self.s(REPORTER_MIN_HEIGHT));
        let inset = if boolean { height * 0.5 } else { self.s(10.0) };
        vec2(content.x + 2.0 * inset, height)
    }

    // ---- Stacks and blocks ---------------------------------------------------

    /// Draws a stack with its top-left corner at `at`, and returns its
    /// height. `loose` is true for a stack under no event, whose top block
    /// shows the notch a block above would fit into. A block grabbed this
    /// frame is taken out of the stack, with the blocks under it.
    pub(super) fn stack(
        &mut self,
        ui: &mut egui::Ui,
        blocks: &mut Vec<Block>,
        at: Pos2,
        loose: bool,
    ) -> f32 {
        let mut y = at.y;
        let mut grabbed = None;
        let count = blocks.len();
        for (index, block) in blocks.iter_mut().enumerate() {
            let drawn = self.statement(ui, block, pos2(at.x, y), loose && index == 0);
            if let Some(grab) = drawn.grabbed {
                grabbed = Some((index, grab));
            }
            y += drawn.height;
            if self.interactive && !block.kind.is_cap() {
                self.stack_spots.push((
                    pos2(at.x, y),
                    StackSpot::After {
                        block: block.id,
                        last: index + 1 == count,
                    },
                ));
            }
        }
        if let Some((index, grab)) = grabbed {
            let tail = blocks.split_off(index);
            self.started = Some(Drag {
                item: Dragged::Stack(tail),
                grab,
                size: Vec2::ZERO,
            });
        }
        y - at.y
    }

    /// Draws an event block and the stack under it. Returns the height of
    /// both, and whether the event block was grabbed.
    pub(super) fn script(&mut self, ui: &mut egui::Ui, block: &mut Block, at: Pos2) -> Drawn {
        if let BlockKind::Unattached { body } = &mut block.kind {
            let height = self.stack(ui, body, at, true);
            return Drawn {
                height,
                width: 0.0,
                grabbed: None,
            };
        }
        let drawn = self.hat(ui, block, at);
        let body_top = pos2(at.x, at.y + drawn.height);
        // Top-level Lua has nothing under it.
        let has_body = !block.kind.bodies().is_empty();
        if self.interactive && has_body {
            self.stack_spots.push((
                body_top,
                StackSpot::Mouth {
                    block: block.id,
                    mouth: 0,
                    empty: block
                        .kind
                        .bodies()
                        .first()
                        .is_none_or(|body| body.is_empty()),
                },
            ));
        }
        let mut height = drawn.height;
        if let Some(body) = block.kind.bodies_mut().into_iter().next() {
            height += self.stack(ui, body, body_top, false);
        }
        Drawn { height, ..drawn }
    }

    /// The row of an event block: a flat top with rounder corners, a
    /// lighter band across it, and its category's icon first.
    pub(super) fn hat(&mut self, ui: &mut egui::Ui, block: &mut Block, at: Pos2) -> Drawn {
        let category = block_category(&block.kind);
        let (fill, edge) = {
            let palette = theme::palette(ui.ctx());
            (category.fill(&palette), category.edge(&palette))
        };
        let id = self.id.with(block.id);
        let block_id = block.id;
        let highlighted = self.highlighted == Some(block_id);
        let has_body = !block.kind.bodies().is_empty();
        let mut sections = sections(&mut block.kind, self.objects);
        let Some(Section::Row(parts)) = sections.first_mut() else {
            return Drawn {
                height: 0.0,
                width: 0.0,
                grabbed: None,
            };
        };
        if !matches!(parts.first(), Some(Part::Icon(_))) {
            parts.insert(0, Part::Icon(category.icon()));
        }
        let size = self.row_size(ui, parts);
        let width = size.x.max(self.s(MIN_WIDTH + 40.0));
        let cap = self.s(CAP);
        let row = Rect::from_min_size(pos2(at.x, at.y + cap), vec2(width, size.y));
        let whole = Rect::from_min_max(at, row.max);
        let background = self.painter.add(Shape::Noop);
        let response = self.interact(ui, whole, id.with("hat"));
        self.draw_row(ui, parts, row, id, block_id);
        drop(sections);

        let (top, corner) = (self.s(HAT_CORNER), self.s(CORNER));
        let rounding = CornerRadius {
            nw: radius(top),
            ne: radius(top),
            sw: radius(corner),
            se: radius(corner),
        };
        let band = Rect::from_min_max(at, pos2(whole.right(), at.y + cap + self.s(4.0)));
        let fill_shapes = vec![
            Shape::rect_filled(whole.translate(vec2(0.0, self.s(DROP))), rounding, edge),
            Shape::rect_filled(whole, rounding, fill),
            Shape::rect_filled(
                band,
                CornerRadius {
                    nw: radius(top),
                    ne: radius(top),
                    sw: 0,
                    se: 0,
                },
                Color32::from_white_alpha(34),
            ),
            Shape::closed_line(
                rounded(
                    &[
                        (whole.left_top(), top),
                        (whole.right_top(), top),
                        (whole.right_bottom(), corner),
                        (whole.left_bottom(), corner),
                    ],
                    None,
                    None,
                ),
                self.outline(edge, highlighted),
            ),
        ];
        self.painter.set(background, Shape::Vec(fill_shapes));
        if has_body {
            self.tabs.push(Tab {
                at: pos2(at.x, row.bottom()),
                fill,
                edge,
            });
        }

        let grabbed = self.grab(&response, at, block_id, false);
        Drawn {
            height: cap + size.y,
            width,
            grabbed,
        }
    }

    /// A statement block, with any blocks in its mouths.
    pub(super) fn statement(
        &mut self,
        ui: &mut egui::Ui,
        block: &mut Block,
        at: Pos2,
        loose: bool,
    ) -> Drawn {
        let category = block_category(&block.kind);
        let (fill, edge) = {
            let palette = theme::palette(ui.ctx());
            (category.fill(&palette), category.edge(&palette))
        };
        let id = self.id.with(block.id);
        let block_id = block.id;
        let cap = block.kind.is_cap();
        let grows = block.kind.input_list_mut().is_some();
        let highlighted = self.highlighted == Some(block_id);
        let objects = self.objects;
        let mut sections = sections(&mut block.kind, objects);

        let mut width = self.s(MIN_WIDTH);
        let mut row_heights = Vec::new();
        for section in &mut sections {
            if let Section::Row(parts) = section {
                let size = self.row_size(ui, parts);
                width = width.max(size.x);
                row_heights.push(size.y);
            }
        }
        let has_mouth = sections
            .iter()
            .any(|section| matches!(section, Section::Mouth(_)));

        let background = self.painter.add(Shape::Noop);
        let mut responses = Vec::new();
        // Each piece: its rect, and whether it's a mouth.
        let mut pieces: Vec<(Rect, bool)> = Vec::new();
        let mut y = at.y;
        let (mut row, mut mouth) = (0, 0);
        for section in &mut sections {
            match section {
                Section::Row(parts) => {
                    let rect = Rect::from_min_size(pos2(at.x, y), vec2(width, row_heights[row]));
                    responses.push(self.interact(ui, rect, id.with(("row", row))));
                    self.draw_row(ui, parts, rect, id.with(row), block_id);
                    pieces.push((rect, false));
                    y += rect.height();
                    row += 1;
                }
                Section::Mouth(body) => {
                    let top = pos2(at.x + self.s(ARM), y);
                    if self.interactive {
                        self.stack_spots.push((
                            top,
                            StackSpot::Mouth {
                                block: block_id,
                                mouth,
                                empty: body.is_empty(),
                            },
                        ));
                    }
                    let inside = self.stack(ui, body, top, false).max(self.s(MOUTH_MIN));
                    let arm = Rect::from_min_size(pos2(at.x, y), vec2(self.s(ARM), inside));
                    responses.push(self.interact(ui, arm, id.with(("arm", mouth))));
                    pieces.push((arm, true));
                    // The tab hanging into the mouth, which the first block
                    // inside fits onto.
                    self.tabs.push(Tab {
                        at: top,
                        fill,
                        edge,
                    });
                    y += inside;
                    mouth += 1;
                }
            }
        }
        drop(sections);
        if has_mouth {
            let foot = Rect::from_min_size(pos2(at.x, y), vec2(width, self.s(FOOT)));
            responses.push(self.interact(ui, foot, id.with("foot")));
            pieces.push((foot, false));
            y += foot.height();
        }

        let shapes = self.block_shapes(&pieces, fill, edge, highlighted);
        self.painter.set(background, Shape::Vec(shapes));
        if !cap {
            self.tabs.push(Tab {
                at: pos2(at.x, y),
                fill,
                edge,
            });
        }
        if loose {
            self.notches.push(Tab { at, fill, edge });
        }

        let mut grabbed = None;
        for response in &responses {
            if let Some(grab) = self.grab(response, at, block_id, grows) {
                grabbed = Some(grab);
            }
        }
        Drawn {
            height: y - at.y,
            width,
            grabbed,
        }
    }

    /// Senses dragging and right-clicks on part of a block.
    pub(super) fn interact(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        id: egui::Id,
    ) -> Option<egui::Response> {
        self.interactive.then(|| {
            ui.interact(rect, id, Sense::click_and_drag())
                .on_hover_cursor(CursorIcon::Grab)
        })
    }

    /// If `response` started a drag, the offset from the block's top-left
    /// corner `at` to the pointer. Also shows the block's right-click menu.
    pub(super) fn grab(
        &mut self,
        response: &Option<egui::Response>,
        at: Pos2,
        block: u64,
        grows: bool,
    ) -> Option<Vec2> {
        let response = response.as_ref()?;
        response.context_menu(|ui| {
            if grows {
                if ui.button("Add Input").clicked() {
                    self.actions.push(Action::AddInput(block));
                    ui.close();
                }
                if ui.button("Remove Last Input").clicked() {
                    self.actions.push(Action::RemoveInput(block));
                    ui.close();
                }
            }
            if ui
                .button(theme::with_icon(icons::COPY, "Duplicate"))
                .clicked()
            {
                self.actions.push(Action::Duplicate(block));
                ui.close();
            }
            if ui
                .button(theme::with_icon(icons::TRASH, "Delete"))
                .clicked()
            {
                self.actions.push(Action::Delete(block));
                ui.close();
            }
        });
        if response.drag_started() {
            let origin = response.interact_pointer_pos()?;
            return Some(origin - at);
        }
        None
    }

    pub(super) fn outline(&self, edge: Color32, highlighted: bool) -> Stroke {
        if highlighted {
            Stroke::new(self.s(3.0).max(2.0), ERROR_OUTLINE)
        } else {
            Stroke::new(1.0_f32, edge)
        }
    }

    /// Fills a block's pieces (rows, arms, foot) and outlines them as one shape.
    pub(super) fn block_shapes(
        &self,
        pieces: &[(Rect, bool)],
        fill: Color32,
        edge: Color32,
        highlighted: bool,
    ) -> Vec<Shape> {
        let (corner, inner) = (self.s(CORNER), self.s(INNER_CORNER));
        let Some(&(first, _)) = pieces.first() else {
            return Vec::new();
        };
        let (left, right) = (first.left(), first.right());
        let last = pieces.len() - 1;
        let mut shapes = Vec::new();
        // The shadow: the rows a little lower in the darker shade, showing
        // under the block and inside its mouths.
        let drop = vec2(0.0, self.s(DROP));
        for (index, &(rect, arm)) in pieces.iter().enumerate() {
            if !arm {
                shapes.push(Shape::rect_filled(
                    rect.translate(drop),
                    self.piece_rounding(index, last, corner, inner),
                    edge,
                ));
            }
        }
        for (index, &(rect, arm)) in pieces.iter().enumerate() {
            if arm {
                // A little longer, so no seam shows where it meets the rows.
                shapes.push(Shape::rect_filled(
                    rect.expand2(vec2(0.0, 1.0)),
                    CornerRadius::ZERO,
                    fill,
                ));
                continue;
            }
            let rounding = self.piece_rounding(index, last, corner, inner);
            shapes.push(Shape::rect_filled(rect, rounding, fill));
        }

        // The outline goes clockwise from the top-left corner: along the
        // top, down the right side, in and out of each mouth, then back
        // along the bottom and up the left side.
        let mut corners = vec![(first.left_top(), corner), (first.right_top(), corner)];
        for (index, &(rect, arm)) in pieces.iter().enumerate() {
            if arm {
                let inside = rect.right();
                corners.push((pos2(right, rect.top()), inner));
                corners.push((pos2(inside, rect.top()), inner));
                corners.push((pos2(inside, rect.bottom()), inner));
                corners.push((pos2(right, rect.bottom()), inner));
            } else if index == last {
                corners.push((pos2(right, rect.bottom()), corner));
                corners.push((pos2(left, rect.bottom()), corner));
            }
        }
        shapes.push(Shape::closed_line(
            rounded(&corners, None, None),
            self.outline(edge, highlighted),
        ));
        shapes
    }

    /// The corners of a block's row or foot: square where it meets an arm.
    fn piece_rounding(&self, index: usize, last: usize, corner: f32, inner: f32) -> CornerRadius {
        let after_mouth = index > 0;
        let before_mouth = index < last;
        CornerRadius {
            nw: if after_mouth { 0 } else { radius(corner) },
            ne: radius(if after_mouth { inner } else { corner }),
            sw: if before_mouth { 0 } else { radius(corner) },
            se: radius(if before_mouth { inner } else { corner }),
        }
    }

    // ---- Rows and their parts -------------------------------------------------

    /// Lays `parts` out along `rect`, centred top to bottom.
    pub(super) fn draw_row(
        &mut self,
        ui: &mut egui::Ui,
        parts: &mut [Part],
        rect: Rect,
        id: egui::Id,
        block: u64,
    ) {
        let address = SlotAddress {
            block,
            path: Vec::new(),
        };
        self.draw_parts(
            ui,
            parts,
            rect.left() + self.s(PAD),
            rect.center().y,
            id,
            &address,
        );
    }

    /// Draws `parts` left to right from `x`, centred on `center_y`. Slots
    /// are numbered from 0 under `address`.
    pub(super) fn draw_parts(
        &mut self,
        ui: &mut egui::Ui,
        parts: &mut [Part],
        mut x: f32,
        center_y: f32,
        id: egui::Id,
        address: &SlotAddress,
    ) {
        let mut slot = 0;
        for (index, part) in parts.iter_mut().enumerate() {
            let size = self.part_size(ui, part);
            let rect = Rect::from_min_size(pos2(x, center_y - size.y * 0.5), size);
            let part_id = id.with(index);
            if let Part::Slot(expression, kind) = part {
                self.slot(ui, expression, *kind, rect, part_id, address.child(slot));
                slot += 1;
            } else {
                // A function's parameters stand for its slot.
                if matches!(part, Part::Params(_)) {
                    slot += 1;
                }
                self.draw_field(ui, part, rect, part_id);
            }
            x += size.x + self.s(GAP);
        }
    }

    /// Draws one part of a block that isn't a value slot (a label, a menu,
    /// grey Lua) in `rect`.
    pub(super) fn draw_field(
        &mut self,
        ui: &mut egui::Ui,
        part: &mut Part,
        rect: Rect,
        part_id: egui::Id,
    ) {
        match part {
            Part::Label(text) => {
                self.painter.text(
                    rect.left_center(),
                    egui::Align2::LEFT_CENTER,
                    text.as_str(),
                    self.font(),
                    Color32::WHITE,
                );
            }
            Part::Icon(icon) => {
                self.painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    *icon,
                    FontId::proportional(16.0 * self.z),
                    Color32::WHITE,
                );
            }
            // Value slots are drawn by `slot`.
            Part::Slot(..) => {}
            Part::Object(path, filter) => {
                let filter = *filter;
                let shown = shown_path(path, filter).to_owned();
                if let Some(response) = self.dropdown(ui, rect, part_id, &shown) {
                    let objects = self.objects;
                    egui::Popup::menu(&response).show(|ui| {
                        object_menu(ui, path, filter, objects);
                    });
                }
            }
            Part::Property(name, class) => {
                let class = *class;
                if let Some(response) = self.dropdown(ui, rect, part_id, name) {
                    egui::Popup::menu(&response).show(|ui| property_menu(ui, name, class));
                }
            }
            Part::Variable(name) => {
                let variables = self.variables;
                if let Some(response) = self.dropdown(ui, rect, part_id, name) {
                    egui::Popup::menu(&response)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .show(|ui| variable_menu(ui, name, variables));
                }
            }
            Part::Key(key) => {
                if let Some(response) = self.dropdown(ui, rect, part_id, key) {
                    egui::Popup::menu(&response).show(|ui| key_menu(ui, key));
                }
            }
            // Children of the object at `parent` (the Workspace when empty).
            Part::Name(name, NameKind::Child, parent) => {
                let prefix = if parent.is_empty() {
                    String::new()
                } else {
                    format!("{parent}.")
                };
                let suggestions: Vec<String> = self
                    .objects
                    .iter()
                    .filter_map(|object| object.path.strip_prefix(prefix.as_str()))
                    .filter(|rest| !rest.is_empty() && !rest.contains('.'))
                    .map(str::to_owned)
                    .collect();
                if let Some(response) = self.dropdown(ui, rect, part_id, name) {
                    egui::Popup::menu(&response)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .show(|ui| name_menu(ui, name, NameKind::Child, &suggestions));
                }
            }
            Part::Name(name, kind, animator) => {
                let (kind, objects) = (*kind, self.objects);
                let object = objects.iter().find(|object| object.path == *animator);
                let suggestions: Vec<String> = match object {
                    // Events come from the class, not the editor's world.
                    Some(object) if kind == NameKind::Signal => object
                        .class
                        .events()
                        .iter()
                        .chain(axcenity_core::INSTANCE_EVENTS.iter())
                        .map(|event| (*event).to_owned())
                        .collect(),
                    Some(object) => object
                        .names
                        .iter()
                        .filter(|(of, _)| *of == kind)
                        .map(|(_, name)| name.clone())
                        .collect(),
                    None => Vec::new(),
                };
                if let Some(response) = self.dropdown(ui, rect, part_id, name) {
                    egui::Popup::menu(&response)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .show(|ui| name_menu(ui, name, kind, &suggestions));
                }
            }
            Part::Math(op) => {
                if let Some(response) = self.dropdown(ui, rect, part_id, op.symbol()) {
                    egui::Popup::menu(&response).show(|ui| {
                        for choice in MathOp::ALL {
                            if ui
                                .selectable_label(**op == choice, choice.symbol())
                                .clicked()
                            {
                                **op = choice;
                                ui.close();
                            }
                        }
                    });
                }
            }
            Part::Compare(op) => {
                if let Some(response) = self.dropdown(ui, rect, part_id, op.symbol()) {
                    egui::Popup::menu(&response).show(|ui| {
                        for choice in Comparison::ALL {
                            if ui
                                .selectable_label(**op == choice, choice.symbol())
                                .clicked()
                            {
                                **op = choice;
                                ui.close();
                            }
                        }
                    });
                }
            }
            Part::Logic(op) => {
                if let Some(response) = self.dropdown(ui, rect, part_id, op.lua()) {
                    egui::Popup::menu(&response).show(|ui| {
                        for choice in [LogicOp::And, LogicOp::Or] {
                            if ui.selectable_label(**op == choice, choice.lua()).clicked() {
                                **op = choice;
                                ui.close();
                            }
                        }
                    });
                }
            }
            Part::Text(text) | Part::Params(text) => {
                self.typed_box(ui, rect, part_id, text);
            }
            Part::Code(code) => {
                let galley = self.code_galley(ui, code);
                self.painter.rect_filled(
                    rect,
                    self.s(4.0),
                    Color32::from_rgba_unmultiplied(0, 0, 0, 60),
                );
                self.painter.galley(
                    rect.min + vec2(self.s(6.0), self.s(4.0)),
                    galley,
                    Color32::WHITE,
                );
            }
            Part::Choice(name, choices) => {
                let choices = *choices;
                if let Some(response) = self.dropdown(ui, rect, part_id, name) {
                    egui::Popup::menu(&response).show(|ui| {
                        for &choice in choices {
                            if ui.selectable_label(**name == choice, choice).clicked() {
                                **name = choice.to_owned();
                                ui.close();
                            }
                        }
                    });
                }
            }
            Part::Function(function) => {
                if let Some(response) = self.dropdown(ui, rect, part_id, function.name()) {
                    egui::Popup::menu(&response).show(|ui| {
                        for choice in MathFunction::ALL {
                            if ui
                                .selectable_label(**function == choice, choice.name())
                                .clicked()
                            {
                                **function = choice;
                                ui.close();
                            }
                        }
                    });
                }
            }
        }
    }

    /// A box to type a name or some Lua in, on a block.
    pub(super) fn typed_box(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        id: egui::Id,
        text: &mut String,
    ) -> Option<egui::Response> {
        self.painter.rect_filled(
            rect,
            radius(self.s(FIELD_CORNER)),
            Color32::from_black_alpha(45),
        );
        if self.interactive {
            return Some(
                ui.put(
                    rect,
                    egui::TextEdit::singleline(text)
                        .id(id)
                        // No box: the block is drawn behind it (egui's usual margin kept).
                        .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(4, 2)))
                        .font(self.font())
                        .text_color(Color32::WHITE)
                        .horizontal_align(egui::Align::Center)
                        .vertical_align(egui::Align::Center)
                        .min_size(rect.size())
                        .margin(egui::Margin::ZERO),
                ),
            );
        }
        self.painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text.as_str(),
            self.font(),
            Color32::WHITE,
        );
        None
    }

    /// A menu's button on a block: darker than the block, with an arrow.
    pub(super) fn dropdown(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        id: egui::Id,
        text: &str,
    ) -> Option<egui::Response> {
        let response = self
            .interactive
            .then(|| ui.interact(rect, id, Sense::click()));
        let hovered = response.as_ref().is_some_and(egui::Response::hovered);
        let fill = Color32::from_black_alpha(if hovered { 70 } else { 45 });
        self.painter
            .rect_filled(rect, radius(self.s(FIELD_CORNER)), fill);
        self.painter.text(
            pos2(rect.left() + self.s(9.0), rect.center().y),
            egui::Align2::LEFT_CENTER,
            text,
            self.font(),
            Color32::WHITE,
        );
        self.painter.text(
            pos2(rect.right() - self.s(9.0), rect.center().y),
            egui::Align2::CENTER_CENTER,
            icons::CARET_DOWN,
            FontId::proportional(10.0 * self.z),
            Color32::WHITE,
        );
        response
    }

    /// A value slot: a value typed in, or a reporter dropped in.
    pub(super) fn slot(
        &mut self,
        ui: &mut egui::Ui,
        expression: &mut Expression,
        kind: SlotKind,
        rect: Rect,
        id: egui::Id,
        address: SlotAddress,
    ) {
        if expression.is_literal() {
            if self.interactive {
                self.slot_spots.push((rect, address, kind));
            }
            self.literal(ui, expression, kind, rect, id);
        } else {
            self.reporter(ui, expression, rect.min, id, Some((address, kind)));
        }
    }

    /// Draws a reporter with its top-left corner at `at`, and returns its
    /// size. In a slot (`slot`), grabbing it takes it out of the slot.
    pub(super) fn reporter(
        &mut self,
        ui: &mut egui::Ui,
        expression: &mut Expression,
        at: Pos2,
        id: egui::Id,
        slot: Option<(SlotAddress, SlotKind)>,
    ) -> Vec2 {
        let size = self.reporter_size(ui, expression);
        let rect = Rect::from_min_size(at, size);
        if self.interactive
            && let Some((_, kind)) = &slot
        {
            let response = ui
                .interact(rect, id.with("reporter"), Sense::click_and_drag())
                .on_hover_cursor(CursorIcon::Grab);
            if response.drag_started()
                && let Some(origin) = response.interact_pointer_pos()
            {
                let taken = std::mem::replace(expression, kind.empty());
                self.started = Some(Drag {
                    item: Dragged::Reporter(taken),
                    grab: origin - at,
                    size,
                });
                return size;
            }
        }
        let category = reporter_category(expression);
        let (fill, edge) = {
            let palette = theme::palette(ui.ctx());
            (category.fill(&palette), category.edge(&palette))
        };
        let boolean = expression.shape() == BlockShape::Boolean;
        if boolean {
            self.painter.add(Shape::convex_polygon(
                hexagon(rect),
                fill,
                Stroke::new(1.0_f32, edge),
            ));
        } else {
            self.painter.add(egui::epaint::RectShape::new(
                rect,
                radius(self.s(FIELD_CORNER + 2.0)),
                fill,
                Stroke::new(1.0_f32, edge),
                egui::StrokeKind::Inside,
            ));
        }
        let inset = if boolean { size.y * 0.45 } else { self.s(10.0) };
        let address = slot.map_or(
            SlotAddress {
                block: 0,
                path: Vec::new(),
            },
            |(address, _)| address,
        );
        let objects = self.objects;
        let mut parts = reporter_parts(expression, objects);
        // Slots inside a reporter that isn't in a block (the palette, or
        // one being dragged) aren't places to drop into.
        let interactive = self.interactive;
        if address.path.is_empty() {
            self.interactive = false;
        }
        self.draw_parts(ui, &mut parts, at.x + inset, rect.center().y, id, &address);
        self.interactive = interactive;
        size
    }

    /// The light field a typed value sits in.
    pub(super) fn field(&self, rect: Rect) {
        self.painter.add(egui::epaint::RectShape::new(
            rect,
            radius(self.s(FIELD_CORNER)),
            INPUT_FIELD,
            Stroke::new(1.0_f32, Color32::from_black_alpha(36)),
            egui::StrokeKind::Inside,
        ));
    }

    /// A value typed into a slot.
    pub(super) fn literal(
        &mut self,
        ui: &mut egui::Ui,
        expression: &mut Expression,
        kind: SlotKind,
        rect: Rect,
        id: egui::Id,
    ) {
        let font = self.font();
        let response = match expression {
            Expression::Number { value } => {
                self.field(rect);
                self.number_input(ui, rect, id, value, font)
            }
            Expression::Text { value } => {
                self.field(rect);
                if self.interactive {
                    Some(
                        ui.put(
                            rect,
                            egui::TextEdit::singleline(value)
                                .id(id)
                                // No box: the block is drawn behind it (egui's usual margin kept).
                                .frame(
                                    egui::Frame::NONE.inner_margin(egui::Margin::symmetric(4, 2)),
                                )
                                .font(font)
                                .text_color(INPUT_TEXT)
                                .horizontal_align(egui::Align::Center)
                                .vertical_align(egui::Align::Center)
                                .min_size(rect.size())
                                .margin(egui::Margin::ZERO),
                        ),
                    )
                } else {
                    self.painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        value.as_str(),
                        font,
                        INPUT_TEXT,
                    );
                    None
                }
            }
            Expression::Boolean { value } if kind == SlotKind::Condition => {
                // An empty pointed hole that still says what it's set to.
                self.painter.add(Shape::convex_polygon(
                    hexagon(rect),
                    Color32::from_black_alpha(60),
                    Stroke::NONE,
                ));
                self.painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    truth_name(*value),
                    FontId::proportional(10.5 * self.z),
                    Color32::from_white_alpha(170),
                );
                let response = self
                    .interactive
                    .then(|| ui.interact(rect, id, Sense::click()));
                if let Some(response) = &response
                    && response.clicked()
                {
                    *value = !*value;
                }
                response.map(|response| {
                    response.on_hover_text(
                        "Drop a pointed block here, or click to switch true and false",
                    )
                })
            }
            Expression::Boolean { value } => {
                let text = truth_name(*value);
                let response = self.dropdown(ui, rect, id, text);
                if let Some(response) = &response {
                    egui::Popup::menu(response).show(|ui| {
                        for choice in [true, false] {
                            if ui
                                .selectable_label(*value == choice, truth_name(choice))
                                .clicked()
                            {
                                *value = choice;
                                ui.close();
                            }
                        }
                    });
                }
                response
            }
            Expression::Vector3 { x, y, z } => {
                let mut left = rect.left();
                let mut last = None;
                for (axis, value) in [x, y, z].into_iter().enumerate() {
                    let width = self.axis_width(ui, *value);
                    let oval =
                        Rect::from_min_size(pos2(left, rect.top()), vec2(width, rect.height()));
                    self.field(oval);
                    last = self.number_input(ui, oval, id.with(axis), value, font.clone());
                    left += width + self.s(3.0);
                }
                last
            }
            Expression::Color3 { r, g, b } => {
                let mut srgb =
                    [*r, *g, *b].map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
                self.field(rect);
                let swatch = rect.shrink(self.s(3.0));
                if self.interactive {
                    // `Color3` is sRGB from 0 to 1, as the colour button's bytes are.
                    let response = ui
                        .scope_builder(egui::UiBuilder::new().max_rect(swatch), |ui| {
                            ui.spacing_mut().interact_size = swatch.size();
                            crate::color_palette::edit_srgb(ui, &mut srgb)
                        })
                        .inner;
                    if response.changed() {
                        [*r, *g, *b] = srgb.map(|channel| f64::from(channel) / 255.0);
                    }
                    Some(response)
                } else {
                    let [red, green, blue] = srgb;
                    self.painter.rect_filled(
                        swatch,
                        radius(self.s(FIELD_CORNER - 2.0)),
                        Color32::from_rgb(red, green, blue),
                    );
                    None
                }
            }
            Expression::Name { name } => self.typed_box(ui, rect, id, name),
            Expression::Nil | Expression::Vararg => {
                let word = if matches!(expression, Expression::Nil) {
                    "nil"
                } else {
                    "..."
                };
                self.painter
                    .rect_filled(rect, radius(self.s(4.0)), Color32::from_black_alpha(45));
                self.painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    word,
                    font,
                    Color32::from_white_alpha(170),
                );
                self.interactive
                    .then(|| ui.interact(rect, id, Sense::click()))
            }
            Expression::Closure { params, .. } => {
                // Its blocks are in a mouth of the block it's in, or, if
                // it's deep in another value, on the Graph view's node.
                self.painter
                    .rect_filled(rect, radius(self.s(4.0)), Color32::from_black_alpha(45));
                self.painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    closure_label(params),
                    font,
                    Color32::WHITE,
                );
                None
            }
            _ => None,
        };
        // Any value slot can hold any kind of value: right-click to choose.
        if kind == SlotKind::Any
            && let Some(response) = response
        {
            response.context_menu(|ui| {
                ui.weak("Use a…");
                for (name, value) in [
                    ("Number", SlotKind::Number.empty()),
                    ("Text", SlotKind::Text.empty()),
                    ("True or false", SlotKind::Truth.empty()),
                    ("Vector3", SlotKind::Vector.empty()),
                    ("Color", SlotKind::Color.empty()),
                    (
                        "Name",
                        Expression::Name {
                            name: "name".to_owned(),
                        },
                    ),
                    ("Nil", Expression::Nil),
                ] {
                    if ui.button(name).clicked() {
                        *expression = value;
                        ui.close();
                    }
                }
            });
        }
    }

    /// A number typed into a block. What's typed is kept as text while the
    /// box has focus, so a half-typed `1.` isn't turned back into `1`.
    pub(super) fn number_input(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        id: egui::Id,
        value: &mut f64,
        font: FontId,
    ) -> Option<egui::Response> {
        if !self.interactive {
            self.painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format_number(*value),
                font,
                INPUT_TEXT,
            );
            return None;
        }
        let buffer = id.with("typed");
        let mut text = ui
            .data(|data| data.get_temp::<String>(buffer))
            .unwrap_or_else(|| format_number(*value));
        let response = ui.put(
            rect,
            egui::TextEdit::singleline(&mut text)
                .id(id)
                // No box: the block is drawn behind it (egui's usual margin kept).
                .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(4, 2)))
                .font(font)
                .text_color(INPUT_TEXT)
                .horizontal_align(egui::Align::Center)
                .vertical_align(egui::Align::Center)
                .min_size(rect.size())
                .margin(egui::Margin::ZERO),
        );
        if response.changed()
            && let Ok(number) = text.trim().parse::<f64>()
            && number.is_finite()
        {
            *value = number;
        }
        if response.has_focus() {
            ui.data_mut(|data| data.insert_temp(buffer, text));
        } else {
            ui.data_mut(|data| data.remove::<String>(buffer));
        }
        Some(response)
    }

    /// Draws the tabs and notches collected while drawing, over the blocks.
    pub(super) fn finish(&mut self) {
        let drop = vec2(0.0, self.s(DROP));
        for tab in std::mem::take(&mut self.tabs) {
            let points = tab_points(tab.at, self.z);
            // Covers the outline it hangs from, so the two blocks look joined.
            let mut filled = points.clone();
            raise_ends(&mut filled);
            let shadow: Vec<Pos2> = filled.iter().map(|point| *point + drop).collect();
            self.painter
                .add(Shape::convex_polygon(shadow, tab.edge, Stroke::NONE));
            self.painter
                .add(Shape::convex_polygon(filled, tab.fill, Stroke::NONE));
            self.painter
                .add(Shape::line(points, Stroke::new(1.0_f32, tab.edge)));
        }
        for notch in std::mem::take(&mut self.notches) {
            let points = tab_points(notch.at, self.z);
            let mut filled = points.clone();
            raise_ends(&mut filled);
            self.painter
                .add(Shape::convex_polygon(filled, self.background, Stroke::NONE));
            self.painter
                .add(Shape::line(points, Stroke::new(1.0_f32, notch.edge)));
        }
    }
}

/// Lifts a tab's two ends by 1.5 points, so its fill covers the outline of
/// the block it hangs from.
fn raise_ends(points: &mut [Pos2]) {
    let last = points.len().saturating_sub(1);
    for index in [0, last] {
        if let Some(point) = points.get_mut(index) {
            point.y -= 1.5;
        }
    }
}

/// Corner radius as egui stores it.
pub(super) fn radius(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

pub(super) fn truth_name(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

/// A number as blocks show it: no decimal point for whole numbers.
pub(super) fn format_number(value: f64) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    rounded.to_string()
}

/// The rounded tab's outline, left to right, for a block whose left edge
/// and bottom are at `at`: a shallow bump, convex, so it fills as one piece.
pub(super) fn tab_points(at: Pos2, z: f32) -> Vec<Pos2> {
    let left = at.x + TAB_LEFT * z;
    let width = TAB_WIDTH * z;
    let depth = TAB_DEPTH * z;
    let steps = 12;
    (0..=steps)
        .map(|step| {
            let t = step as f32 / steps as f32;
            // A flattened half-sine: steep sides, a broad rounded bottom.
            let dip = (std::f32::consts::PI * t).sin().max(0.0).powf(0.6);
            pos2(left + width * t, at.y + depth * dip)
        })
        .collect()
}

/// The pointed shape of a true-or-false reporter: short points, so it
/// reads as a value with arrowed ends.
pub(super) fn hexagon(rect: Rect) -> Vec<Pos2> {
    let point = rect.height() * 0.4;
    vec![
        pos2(rect.left(), rect.center().y),
        pos2(rect.left() + point, rect.top()),
        pos2(rect.right() - point, rect.top()),
        pos2(rect.right(), rect.center().y),
        pos2(rect.right() - point, rect.bottom()),
        pos2(rect.left() + point, rect.bottom()),
    ]
}

/// The points of a polygon whose corners are rounded by their radii. With
/// `before` and `after`, the path comes from `before` and goes on to
/// `after` (an open run of corners); without, it's closed.
pub(super) fn rounded(
    corners: &[(Pos2, f32)],
    before: Option<Pos2>,
    after: Option<Pos2>,
) -> Vec<Pos2> {
    let count = corners.len();
    let mut points = Vec::new();
    for index in 0..count {
        let (corner, radius) = corners[index];
        let previous = if index == 0 {
            before.unwrap_or(corners[count - 1].0)
        } else {
            corners[index - 1].0
        };
        let next = if index + 1 == count {
            after.unwrap_or(corners[0].0)
        } else {
            corners[index + 1].0
        };
        let into = (previous - corner).normalized();
        let out = (next - corner).normalized();
        let reach = radius
            .min((previous - corner).length() * 0.5)
            .min((next - corner).length() * 0.5);
        if reach < 0.5 || !into.is_finite() || !out.is_finite() {
            points.push(corner);
            continue;
        }
        let start = corner + into * reach;
        let end = corner + out * reach;
        // A quadratic curve through the corner looks like a rounded corner.
        let steps = 5;
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let a = start.lerp(corner, t);
            let b = corner.lerp(end, t);
            points.push(a.lerp(b, t));
        }
    }
    points
}

/// What an object menu shows for `path`: "Workspace" for the empty path of
/// a place (where the Workspace itself can be picked).
fn shown_path(path: &str, filter: Filter) -> &str {
    if path.is_empty() && filter == Filter::Places {
        "Workspace"
    } else {
        path
    }
}

/// How a function in a value slot is shown.
fn closure_label(params: &str) -> String {
    format!("function({params})")
}
