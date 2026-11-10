//! Keyboard and controller navigation. One element at a time has focus
//! (CSS `:focus`): an interactable, `Selectable` element of the last
//! layout. [`NavKey`]s move it, the arrows to the nearest one that way (or
//! the element named in `NextSelectionUp`, `Down`, `Left` or `Right`), Next
//! and Previous through the tree's order, and Confirm uses it as a click
//! would. A ScrollingFrame scrolls to keep it in view.

use std::collections::HashSet;

use axcenity_render::UiRectangle;

use crate::UiRect;

use super::*;

/// The default focus ring's colour, drawn when no `:focus` rule styles the
/// focused element.
pub const FOCUS_RING_COLOR: [f32; 4] = [0.29, 0.62, 1.0, 1.0];
/// How thick the default focus ring is, and how far outside the element.
const RING_WIDTH: f32 = 3.0;
const RING_GAP: f32 = 2.0;

/// A key that moves focus or uses the focused element: from the keyboard
/// (arrows, Tab, Shift+Tab, Enter, Escape) or a controller (see
/// [`crate::nav_key`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NavKey {
    Up,
    Down,
    Left,
    Right,
    /// Tab: the next element in the tree's order, wrapping round.
    Next,
    /// Shift+Tab: the one before.
    Previous,
    /// Enter, or South on a controller: clicks a button, starts typing in
    /// a text box, flips a toggle, opens a dropdown or picks from its list.
    Confirm,
    /// Escape, or East: closes an open list, else drops focus.
    Back,
}

/// What a [`NavKey`] did: [`UiInteraction::navigate`].
#[derive(Clone, Debug, Default, PartialEq)]
#[must_use = "a scroll or a widget's new value only shows once it's applied to the world"]
pub struct UiNavigated {
    /// The UI used the key, so the host doesn't pass it on to the game.
    pub taken: bool,
    /// The ScrollingFrame to set `CanvasPosition` on, to show the focus.
    pub moved: Option<(InstanceId, Vector2)>,
    /// The widget to set `Value` on: a Slider moved, a Toggle flipped or a
    /// Dropdown's option picked.
    pub value: Option<(InstanceId, Value)>,
    /// A button confirmed: the host fires `Activated` as for a click.
    pub activated: Option<InstanceId>,
    /// A TextBox confirmed: the host starts typing in it.
    pub text_box: Option<InstanceId>,
}

impl UiNavigated {
    /// Sets the moved frame's `CanvasPosition` and the changed widget's
    /// `Value` in `world`. Returns [`Self::taken`].
    pub fn apply(&self, world: &mut World) -> bool {
        UiScrolled {
            taken: self.taken,
            moved: self.moved,
            value: self.value.clone(),
        }
        .apply(world)
    }

    fn taken() -> UiNavigated {
        UiNavigated {
            taken: true,
            ..UiNavigated::default()
        }
    }
}

/// Whether navigation can focus `element`: it's interactable and `Selectable`.
pub(super) fn selectable(world: &World, element: &UiElement) -> bool {
    element.interactable
        && world.get(element.id).is_some_and(|instance| {
            instance.property("Selectable").and_then(Value::as_bool) == Some(true)
        })
}

impl UiInteraction {
    /// The element keyboard or controller navigation has focused.
    pub fn selected(&self) -> Option<InstanceId> {
        self.selected
    }

    /// What the focused element is called, as a screen reader would say
    /// it: its `AccessibleName`, else its `Name`.
    pub fn selected_name<'a>(&self, world: &'a World) -> Option<&'a str> {
        world.get(self.selected?)?.accessible_name()
    }

    /// Focuses `id` (or nothing), as navigation would, from the next layout.
    pub fn select(&mut self, id: Option<InstanceId>) {
        self.selected = id;
    }

    /// Whether `key` is for the UI rather than the game, so a host gives it
    /// to [`Self::navigate`] instead of to scripts: Tab and Shift+Tab when
    /// there's something to focus, and the other keys while something is
    /// focused. A controller button only the UI uses always goes to it
    /// ([`crate::PadNavigation`]).
    pub fn takes(&self, world: &World, key: NavKey) -> bool {
        match key {
            NavKey::Next | NavKey::Previous => {
                self.selected.is_some() || !self.tab_order(world).is_empty()
            }
            _ => self.selected.is_some(),
        }
    }

    /// Uses `key`, by the last layout. With nothing focused, Next or an
    /// arrow focuses the first element in the tree's order and Previous the
    /// last. The focused Slider moves with left and right (up and down when
    /// it's vertical) instead of moving focus. While a list opened by Confirm
    /// is showing, up and down choose in it, Confirm picks and Back closes it.
    pub fn navigate(&mut self, world: &World, key: NavKey) -> UiNavigated {
        if let Some((id, _)) = self.dropdown
            && let Some(done) = self.navigate_list(world, id, key)
        {
            return done;
        }
        let current = self
            .selected
            .and_then(|id| self.element(id).copied())
            .filter(|element| selectable(world, element));
        let Some(current) = current else {
            self.selected = None;
            let order = self.tab_order(world);
            let first = match key {
                NavKey::Confirm | NavKey::Back => None,
                NavKey::Previous => order.last().copied(),
                _ => order.first().copied(),
            };
            return first.map_or_else(UiNavigated::default, |id| self.focus(world, id));
        };
        let id = current.id;
        match key {
            NavKey::Back => {
                self.selected = None;
                UiNavigated::taken()
            }
            NavKey::Confirm => self.confirm(world, id),
            NavKey::Next | NavKey::Previous => {
                let order = self.tab_order(world);
                let here = order.iter().position(|other| *other == id).unwrap_or(0);
                let next = if key == NavKey::Next {
                    (here + 1) % order.len()
                } else {
                    (here + order.len() - 1) % order.len()
                };
                self.focus(world, order[next])
            }
            direction => {
                if let Some(slid) = self.nudge_slider(world, id, direction) {
                    return slid;
                }
                match self
                    .named_next(world, id, direction)
                    .or_else(|| self.nearest(world, &current, direction))
                {
                    Some(next) => self.focus(world, next),
                    // Nothing that way: focus stays.
                    None => UiNavigated::taken(),
                }
            }
        }
    }

    /// The elements navigation can focus in the last layout, in the
    /// tree's order.
    fn tab_order(&self, world: &World) -> Vec<InstanceId> {
        let laid: HashSet<InstanceId> = self
            .elements
            .iter()
            .filter(|element| selectable(world, element))
            .map(|element| element.id)
            .collect();
        if laid.is_empty() {
            return Vec::new();
        }
        world
            .descendants_iter(world.workspace())
            .filter(|id| laid.contains(id))
            .collect()
    }

    fn focus(&mut self, world: &World, id: InstanceId) -> UiNavigated {
        self.selected = Some(id);
        UiNavigated {
            moved: self.scroll_into_view(world, id),
            ..UiNavigated::taken()
        }
    }

    /// Scrolls the innermost ScrollingFrame around `id` so `id` is inside it.
    fn scroll_into_view(&mut self, world: &World, id: InstanceId) -> Option<(InstanceId, Vector2)> {
        let child = self.element(id)?.rect;
        let mut current = world.get(id)?.parent();
        let frame = loop {
            let ancestor = current?;
            if let Some(element) = self.element(ancestor)
                && element.scroll.is_some()
            {
                break *element;
            }
            current = world.get(ancestor)?.parent();
        };
        let scroll = frame.scroll?;
        let per_unit = frame.pixels_per_unit.max(1e-4);
        let view = frame.rect;
        let mut to = scroll.position;
        let spans = [
            (child.x, child.width, view.x, view.width),
            (child.y, child.height, view.y, view.height),
        ];
        for (axis, (start, length, view_start, view_length)) in spans.into_iter().enumerate() {
            let before = view_start - start;
            let after = (start + length) - (view_start + view_length);
            if before > 0.0 {
                to[axis] -= before / per_unit;
            } else if after > 0.0 {
                // A child bigger than the frame shows its start.
                to[axis] += after.min(-before) / per_unit;
            }
        }
        self.scroll_to(frame.id, to)
    }

    /// The element `id`'s `NextSelection<direction>` names by its `Id`, when
    /// that's one navigation can focus.
    fn named_next(&self, world: &World, id: InstanceId, direction: NavKey) -> Option<InstanceId> {
        let property = match direction {
            NavKey::Up => "NextSelectionUp",
            NavKey::Down => "NextSelectionDown",
            NavKey::Left => "NextSelectionLeft",
            _ => "NextSelectionRight",
        };
        let target = world.get(id)?.property(property)?.as_str()?.trim();
        if target.is_empty() {
            return None;
        }
        self.tab_order(world).into_iter().find(|&other| {
            other != id
                && world
                    .get(other)
                    .and_then(|instance| instance.property("Id"))
                    .and_then(Value::as_str)
                    == Some(target)
        })
    }

    /// The selectable element nearest `from` in `direction`: its middle is
    /// further that way, and it scores lowest on the gap between them that
    /// way plus twice how far apart they are across it (0 when they
    /// overlap), then on how far apart their middles are.
    fn nearest(&self, world: &World, from: &UiElement, direction: NavKey) -> Option<InstanceId> {
        let a = from.rect;
        let centre = |r: UiRect| [r.x + r.width / 2.0, r.y + r.height / 2.0];
        let ac = centre(a);
        let gap = |start: f32, end: f32, other_start: f32, other_end: f32| {
            (other_start - end).max(start - other_end).max(0.0)
        };
        self.elements
            .iter()
            .filter(|element| element.id != from.id && selectable(world, element))
            .filter_map(|element| {
                let b = element.rect;
                let bc = centre(b);
                let (ahead, along, across) = match direction {
                    NavKey::Down => (
                        bc[1] - ac[1],
                        b.y - (a.y + a.height),
                        gap(a.x, a.x + a.width, b.x, b.x + b.width),
                    ),
                    NavKey::Up => (
                        ac[1] - bc[1],
                        a.y - (b.y + b.height),
                        gap(a.x, a.x + a.width, b.x, b.x + b.width),
                    ),
                    NavKey::Right => (
                        bc[0] - ac[0],
                        b.x - (a.x + a.width),
                        gap(a.y, a.y + a.height, b.y, b.y + b.height),
                    ),
                    _ => (
                        ac[0] - bc[0],
                        a.x - (b.x + b.width),
                        gap(a.y, a.y + a.height, b.y, b.y + b.height),
                    ),
                };
                let apart = (bc[0] - ac[0]).hypot(bc[1] - ac[1]);
                (ahead > 0.5).then_some((along.max(0.0) + across * 2.0, apart, element.id))
            })
            .min_by(|x, y| x.0.total_cmp(&y.0).then(x.1.total_cmp(&y.1)))
            .map(|(_, _, id)| id)
    }

    /// The focused Slider moved one press `direction`, when that's along it.
    fn nudge_slider(
        &self,
        world: &World,
        id: InstanceId,
        direction: NavKey,
    ) -> Option<UiNavigated> {
        let instance = world.get(id)?;
        if instance.class() != ClassName::Slider {
            return None;
        }
        let by = match (widget::vertical(instance), direction) {
            (false, NavKey::Right) | (true, NavKey::Up) => 1.0,
            (false, NavKey::Left) | (true, NavKey::Down) => -1.0,
            _ => return None,
        };
        let value = widget::slider_nudged(instance, by);
        let current = instance.property("Value").and_then(Value::as_number);
        Some(UiNavigated {
            value: (current != Some(value)).then_some((id, Value::Number(value))),
            ..UiNavigated::taken()
        })
    }

    /// Confirm on the focused element: what a click on it does.
    fn confirm(&mut self, world: &World, id: InstanceId) -> UiNavigated {
        let Some(instance) = world.get(id) else {
            return UiNavigated::default();
        };
        match instance.class() {
            ClassName::TextButton => UiNavigated {
                activated: self.activate(world, id).then_some(id),
                ..UiNavigated::taken()
            },
            ClassName::TextBox => UiNavigated {
                text_box: Some(id),
                ..UiNavigated::taken()
            },
            ClassName::Toggle => {
                let on = instance.property("Value").and_then(Value::as_bool) == Some(true);
                UiNavigated {
                    value: Some((id, Value::Bool(!on))),
                    ..UiNavigated::taken()
                }
            }
            ClassName::Dropdown => {
                let options = widget::options(instance);
                if !options.is_empty() {
                    let chosen = instance
                        .property("Value")
                        .and_then(Value::as_str)
                        .and_then(|value| options.iter().position(|option| option == value));
                    self.dropdown = Some((id, None));
                    self.list_cursor = Some(chosen.unwrap_or(0));
                }
                UiNavigated::taken()
            }
            _ => UiNavigated::taken(),
        }
    }

    /// `key` while Dropdown `id`'s list is open. `None` when the key closes
    /// it and carries on as if it hadn't been open (Next and Previous).
    fn navigate_list(&mut self, world: &World, id: InstanceId, key: NavKey) -> Option<UiNavigated> {
        let options = world.get(id).map(widget::options).unwrap_or_default();
        if options.is_empty() {
            self.close_list();
            return None;
        }
        let last = options.len() - 1;
        let chosen = world
            .get(id)
            .and_then(|instance| instance.property("Value"))
            .and_then(Value::as_str)
            .and_then(|value| options.iter().position(|option| option == value));
        let cursor = self.list_cursor.or(chosen);
        match key {
            NavKey::Up | NavKey::Down => {
                let row = match (cursor, key) {
                    (None, _) => 0,
                    (Some(row), NavKey::Up) => row.saturating_sub(1),
                    (Some(row), _) => (row + 1).min(last),
                };
                self.list_cursor = Some(row);
                self.show_row(id, row);
                Some(UiNavigated::taken())
            }
            NavKey::Confirm => {
                let Some(row) = cursor else {
                    return Some(UiNavigated::taken());
                };
                self.close_list();
                let option = options[row.min(last)].clone();
                let current = world
                    .get(id)
                    .and_then(|dropdown| dropdown.property("Value"))
                    .and_then(Value::as_str);
                Some(UiNavigated {
                    value: (current != Some(option.as_str()))
                        .then_some((id, Value::String(option))),
                    ..UiNavigated::taken()
                })
            }
            NavKey::Back => {
                self.close_list();
                Some(UiNavigated::taken())
            }
            NavKey::Left | NavKey::Right => Some(UiNavigated::taken()),
            NavKey::Next | NavKey::Previous => {
                self.close_list();
                None
            }
        }
    }

    /// Scrolls the open list so its `row` shows.
    fn show_row(&mut self, id: InstanceId, row: usize) {
        let Some(list) = &self.list else {
            return;
        };
        let per_unit = list.per_unit.max(1e-4);
        let height = list.row_height / per_unit;
        let showing = (list.rect.height - list.pad * 2.0) / per_unit;
        let top = row as f32 * height;
        let mut scroll = list.scroll;
        if top < scroll {
            scroll = top;
        } else if top + height > scroll + showing {
            scroll = top + height - showing;
        }
        self.dropdown = Some((id, Some(scroll.clamp(0.0, list.max_scroll))));
    }
}

/// The default focus ring around `element`: a blue line just outside its
/// rounded box, turned and clipped with it.
pub(super) fn focus_ring(
    world: &World,
    element: &UiElement,
    stylesheets: &[Stylesheet],
    pseudo: PseudoClasses<'_>,
) -> UiDrawItem {
    let styles = resolve_styles(world, element.id, stylesheets, pseudo);
    let per_unit = element.pixels_per_unit.max(0.5);
    let radius = css_length_property(&styles, "border-radius")
        .unwrap_or(0.0)
        .max(0.0)
        * per_unit;
    let out = RING_GAP + RING_WIDTH;
    let rect = element.rect;
    let mut ring = UiDrawItem::Rectangle(UiRectangle {
        position: [rect.x - out, rect.y - out],
        size: [rect.width + out * 2.0, rect.height + out * 2.0],
        border_color: FOCUS_RING_COLOR,
        border_width: RING_WIDTH,
        border_radius: radius + out,
        clip: element.clip,
        ..UiRectangle::default()
    });
    if let Some(turn) = element.turn {
        crate::turn::turn_draw_item(&mut ring, turn);
    }
    ring
}
