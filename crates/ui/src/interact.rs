//! The pointer and the UI over time: hover and press states, CSS
//! transitions, `MouseEnter` and `MouseLeave`, tooltips, the cursor,
//! buttons' hover and click sounds, scrolling ScrollingFrames, and the
//! widgets' values (dragging a Slider, flipping a Toggle, picking from a
//! Dropdown's list, which it draws over everything while it's open), and
//! keyboard and controller focus (`focus.rs`). Hosts (the player, the
//! editor's viewport) keep one [`UiInteraction`] for a game and lay their
//! UI out through it each frame.

mod focus;

pub use focus::{FOCUS_RING_COLOR, NavKey, UiNavigated};

use std::cell::RefCell;

use axcenity_core::{ClassName, InstanceId, Value, Vector2, World};
use axcenity_render::{Camera, UiDrawItem, UiText};

use crate::css::*;
use crate::style::{
    PseudoClasses, Stylesheet, focus_rule_applies, resolve_styles, resolve_tooltip_styles,
};
use crate::transition::Transitions;
use crate::widget::{self, DropdownList};
use crate::{ScreenGuis, UiElement, UiLayout, layout_ui_with_pseudo, scroll_bars};

/// How long the pointer rests on an element before its tooltip shows, in seconds.
pub const TOOLTIP_DELAY: f64 = 0.5;

/// How far a press on a ScrollingFrame's content moves, in pixels, before
/// it scrolls the frame instead of pressing what's under it.
pub const SCROLL_DRAG_SLOP: f32 = 6.0;

/// A state an element can be shown in: the tabs of the UI Designer's
/// states editor, and the CSS pseudo-class each one writes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum UiState {
    #[default]
    Normal,
    Hover,
    Pressed,
    Disabled,
}

impl UiState {
    pub const ALL: [UiState; 4] = [
        UiState::Normal,
        UiState::Hover,
        UiState::Pressed,
        UiState::Disabled,
    ];

    pub fn name(self) -> &'static str {
        match self {
            UiState::Normal => "Normal",
            UiState::Hover => "Hover",
            UiState::Pressed => "Pressed",
            UiState::Disabled => "Disabled",
        }
    }

    /// What goes after `#Id` for this state's rule: `""`, `:hover`,
    /// `:active` or `:disabled`.
    pub fn pseudo_class(self) -> &'static str {
        match self {
            UiState::Normal => "",
            UiState::Hover => ":hover",
            UiState::Pressed => ":active",
            UiState::Disabled => ":disabled",
        }
    }
}

/// The mouse cursor to show over the UI (CSS `cursor`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiCursor {
    #[default]
    Default,
    /// A hand: buttons, unless their CSS says otherwise.
    Pointer,
    /// An I-beam: text boxes.
    Text,
    NotAllowed,
}

/// Something that happened to the UI, for the host to pass on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiEvent {
    /// Fire `MouseEnter` on this element in LocalScripts.
    MouseEnter(InstanceId),
    /// Fire `MouseLeave` on this element in LocalScripts.
    MouseLeave(InstanceId),
    /// Play this sound file (a project path) once: a button's `HoverSound`
    /// or `ClickSound`.
    PlaySound(String),
}

/// What the pointer and keyboard are doing to the UI this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiInput {
    /// The pointer in physical pixels, or `None` when it's off the screen.
    pub pointer: Option<[f32; 2]>,
    /// The button the left mouse button went down on and is still held.
    pub pressed: Option<InstanceId>,
    /// The TextBox being typed in.
    pub focused: Option<InstanceId>,
    /// A ScreenGui laid out after all the other UI, BillboardGuis too, so it
    /// covers them and takes the pointer first: the game's loading screen.
    pub on_top: Option<InstanceId>,
}

/// The UI's state from one frame to the next. See the module documentation.
#[derive(Debug, Default)]
pub struct UiInteraction {
    transitions: RefCell<Transitions>,
    hovered: Option<InstanceId>,
    /// How long `hovered` has been hovered, in seconds.
    hovered_for: f64,
    events: Vec<UiEvent>,
    /// Where each element was in the last layout, for scrolling and widgets.
    elements: Vec<UiElement>,
    scroll_drag: Option<ScrollDrag>,
    /// The Toggle, Dropdown or list option the left button went down on.
    widget_press: Option<WidgetPress>,
    /// The Dropdown whose list is open, and how far that's scrolled in
    /// authored pixels (`None` until it's first drawn, to show the chosen option).
    dropdown: Option<(InstanceId, Option<f32>)>,
    /// That list, as the last layout drew it.
    list: Option<DropdownList>,
    /// The element keyboard or controller navigation focused (`:focus`).
    selected: Option<InstanceId>,
    /// The option of the open list the keyboard or controller is on, when
    /// it was opened that way.
    list_cursor: Option<usize>,
    /// Lay out only this ScreenGui (see [`UiInteraction::only`]).
    only: Option<InstanceId>,
}

/// A press on a widget that does something when it's let go over the same place.
#[derive(Clone, Copy, Debug, PartialEq)]
enum WidgetPress {
    /// A Toggle: flips it.
    Flip(InstanceId),
    /// A Dropdown: opens its list.
    Open(InstanceId),
    /// An option in a Dropdown's open list, by its index: picks it.
    Pick(InstanceId, usize),
}

/// What the pointer did to the ScrollingFrames and widgets:
/// [`UiInteraction::wheel`], [`UiInteraction::press`],
/// [`UiInteraction::drag`] and [`UiInteraction::release`].
#[derive(Clone, Debug, Default, PartialEq)]
#[must_use = "a scroll or a widget's new value only shows once it's applied to the world"]
pub struct UiScrolled {
    /// The UI took the wheel or the press, so the host doesn't use it for
    /// anything else: the camera doesn't zoom, and no button is pressed.
    pub taken: bool,
    /// The ScrollingFrame to set `CanvasPosition` on, and what to.
    pub moved: Option<(InstanceId, Vector2)>,
    /// The widget to set `Value` on, and what to: a Slider dragged, a
    /// Toggle flipped or a Dropdown's option picked. Setting it fires
    /// `ValueChanged` in scripts.
    pub value: Option<(InstanceId, Value)>,
}

impl UiScrolled {
    /// Sets the moved frame's `CanvasPosition` and the changed widget's
    /// `Value` in `world`. Returns [`Self::taken`].
    pub fn apply(self, world: &mut World) -> bool {
        // An element removed since the last layout has nothing to set.
        if let Some((id, at)) = self.moved {
            let _ = world.set_property(id, "CanvasPosition", Value::Vector2(at));
        }
        if let Some((id, value)) = self.value {
            let _ = world.set_property(id, "Value", value);
        }
        self.taken
    }

    fn taken() -> UiScrolled {
        UiScrolled {
            taken: true,
            ..UiScrolled::default()
        }
    }
}

/// A ScrollingFrame being scrolled by the pointer.
#[derive(Clone, Copy, Debug)]
enum ScrollDrag {
    /// Its thumb on `axis`, held `grab` pixels from the thumb's start.
    Bar {
        id: InstanceId,
        axis: usize,
        grab: f32,
    },
    /// A Slider's thumb, which follows the pointer along its track.
    Slider { id: InstanceId },
    /// Its content, pressed at `from` when it was scrolled to `start`.
    /// It scrolls once the pointer has moved [`SCROLL_DRAG_SLOP`].
    Content {
        id: InstanceId,
        from: [f32; 2],
        start: [f32; 2],
        moving: bool,
    },
}

impl UiInteraction {
    /// UI state for just the ScreenGui `gui`, wherever it is, laid out on
    /// the whole screen: an editor plugin's dock tab, whose ScreenGui is
    /// outside the tree.
    pub fn only(gui: InstanceId) -> Self {
        Self {
            only: Some(gui),
            ..Self::default()
        }
    }

    /// Lays the UI out for this frame, `seconds` after the last one: as
    /// [`crate::layout_ui_interactive`] does, but with styles eased by their
    /// transitions, the hovered element's tooltip drawn, and the cursor to
    /// show. A change of hovered element is queued for [`Self::take_events`].
    pub fn layout(
        &mut self,
        world: &World,
        screen: (u32, u32),
        stylesheets: &[Stylesheet],
        input: UiInput,
        camera: Option<&Camera>,
        seconds: f32,
    ) -> UiLayout {
        let (width, height) = screen;
        self.transitions
            .borrow_mut()
            .advance(world, f64::from(seconds.max(0.0)));
        let mut base = layout_ui_with_pseudo(
            world,
            width,
            height,
            stylesheets,
            PseudoClasses {
                focused: input.focused,
                selected: self.selected,
                ..PseudoClasses::default()
            },
            camera,
            ScreenGuis {
                on_top: input.on_top,
                only: self.only,
            },
        );
        // An open Dropdown's list covers everything under it. Where it is
        // comes from this layout, so it's right on the frame it opens.
        let open = (&mut self.dropdown, &mut self.list, self.list_cursor);
        draw_list(world, screen, stylesheets, input.pointer, open, &mut base);
        // Focus goes when its element is hidden, removed or turned off.
        if let Some(id) = self.selected
            && !base
                .element(id)
                .is_some_and(|element| focus::selectable(world, element))
        {
            self.selected = None;
        }
        let on_list = self
            .list
            .as_ref()
            .is_some_and(|list| input.pointer.is_some_and(|[x, y]| list.rect.contains(x, y)));
        let pointer = input.pointer.filter(|_| !on_list);
        let under = pointer.and_then(|[x, y]| base.hit(x, y));
        let hovered = pointer.and_then(|[x, y]| base.hover_target(x, y));
        let active = input
            .pressed
            .or_else(|| self.held())
            .filter(|button| hovered == Some(*button));
        let pseudo = PseudoClasses {
            hovered,
            active,
            focused: input.focused,
            selected: self.selected,
            forced: None,
            transitions: Some(&self.transitions),
            scrolled: None,
        };
        let mut layout = layout_ui_with_pseudo(
            world,
            width,
            height,
            stylesheets,
            pseudo,
            camera,
            ScreenGuis {
                on_top: input.on_top,
                only: self.only,
            },
        );
        layout.hovered = hovered;
        layout.active = active;
        // The default focus ring, over the UI but under an open list.
        let ring = self
            .selected
            .and_then(|id| layout.element(id))
            .filter(|element| !focus_rule_applies(world, element.id, stylesheets, pseudo))
            .map(|element| focus::focus_ring(world, element, stylesheets, pseudo));
        layout.draw_items.extend(ring);
        // Only these fields, since `pseudo` borrows the transitions.
        let open = (&mut self.dropdown, &mut self.list, self.list_cursor);
        draw_list(world, screen, stylesheets, input.pointer, open, &mut layout);

        if hovered != self.hovered {
            if let Some(old) = self.hovered {
                self.events.push(UiEvent::MouseLeave(old));
            }
            if let Some(new) = hovered {
                self.events.push(UiEvent::MouseEnter(new));
                if let Some(sound) = button_sound(world, new, "HoverSound") {
                    self.events.push(UiEvent::PlaySound(sound));
                }
            }
            self.hovered = hovered;
            self.hovered_for = 0.0;
        } else {
            self.hovered_for += f64::from(seconds.max(0.0));
        }

        if let (Some(id), Some(pointer)) = (hovered, input.pointer)
            && let Some(tip) = tooltip_text(world, id)
        {
            if self.hovered_for >= TOOLTIP_DELAY {
                let mut items = tooltip_items(
                    world,
                    id,
                    stylesheets,
                    &tip,
                    pointer,
                    (width as f32, height as f32),
                );
                // Its text is already scaled, so only contrast is left.
                let settings = axcenity_core::UiSettings {
                    text_scale: 1.0,
                    ..world.ui_settings()
                };
                crate::accessibility::adjust_items(settings, &mut items);
                layout.draw_items.extend(items);
            } else {
                // Nothing moves while the pointer rests, so keep drawing
                // until the tooltip is due.
                layout.animating = true;
            }
        }
        layout.cursor = if on_list {
            UiCursor::Pointer
        } else {
            under.map_or(UiCursor::Default, |id| {
                cursor(world, id, stylesheets, pseudo)
            })
        };
        layout.animating |= self.transitions.borrow().running();
        self.elements.clone_from(&layout.elements);
        layout
    }

    /// The Dropdown whose list is open, if one is.
    pub fn open_dropdown(&self) -> Option<InstanceId> {
        self.dropdown.map(|(id, _)| id)
    }

    fn close_list(&mut self) {
        self.dropdown = None;
        self.list = None;
        self.list_cursor = None;
    }

    /// The widget the left button is held on: a Slider being dragged, or
    /// a Toggle or Dropdown pressed. It's `:active` while it's hovered.
    fn held(&self) -> Option<InstanceId> {
        match (self.scroll_drag, self.widget_press) {
            (Some(ScrollDrag::Slider { id }), _) => Some(id),
            (_, Some(WidgetPress::Flip(id) | WidgetPress::Open(id))) => Some(id),
            _ => None,
        }
    }

    /// The topmost element at `pointer` in the last layout.
    fn top_at(&self, [x, y]: [f32; 2]) -> Option<&UiElement> {
        self.elements.iter().rev().find(|e| e.contains(x, y))
    }

    /// The Slider `id`'s value with its thumb at `pointer`, when that's a change.
    fn slide(
        &self,
        world: &World,
        id: InstanceId,
        pointer: [f32; 2],
    ) -> Option<(InstanceId, Value)> {
        let element = self.element(id)?;
        let track = element.track?;
        let instance = world.get(id)?;
        let local = element.local(pointer);
        let fraction = track.fraction_at(element.rect, element.pixels_per_unit, local);
        let value = widget::slider_value_at(instance, fraction);
        let current = instance.property("Value").and_then(Value::as_number);
        (current != Some(value)).then_some((id, Value::Number(value)))
    }

    /// Takes where everything is from `layout`, for scrolling, when it
    /// wasn't laid out by [`Self::layout`]: the UI Designer's artboard.
    pub fn remember(&mut self, layout: &UiLayout) {
        self.elements.clone_from(&layout.elements);
    }

    /// The ScrollingFrames a point is in, innermost first: the top element
    /// under it and the ScrollingFrames around that, from the last layout.
    fn scrollers_at(&self, world: &World, [x, y]: [f32; 2]) -> Vec<UiElement> {
        let Some(top) = self.elements.iter().rev().find(|e| e.contains(x, y)) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        let mut current = Some(top.id);
        while let Some(id) = current {
            if let Some(element) = self.element(id)
                && element.scroll.is_some()
            {
                found.push(*element);
            }
            current = world.get(id).and_then(|instance| instance.parent());
        }
        found
    }

    fn element(&self, id: InstanceId) -> Option<&UiElement> {
        self.elements.iter().find(|element| element.id == id)
    }

    /// Scrolls `id` to `position` (authored pixels, kept in range) in what
    /// this remembers of the last layout, and says what to set in the
    /// world. `None` when that's where it already is.
    fn scroll_to(&mut self, id: InstanceId, position: [f32; 2]) -> Option<(InstanceId, Vector2)> {
        let element = self.elements.iter_mut().find(|element| element.id == id)?;
        let scroll = element.scroll.as_mut()?;
        let position = scroll.clamp(position);
        if position == scroll.position {
            return None;
        }
        scroll.position = position;
        Some((id, Vector2::new(position[0], position[1])))
    }

    /// The mouse wheel turned by `delta` (pixels across and down; down
    /// scrolls the content up) with the pointer at `pointer`: scrolls the
    /// innermost ScrollingFrame there that can move that way. A frame that
    /// only scrolls across takes the wheel's up and down. The wheel is
    /// taken whenever the pointer is over a ScrollingFrame.
    pub fn wheel(&mut self, world: &World, pointer: [f32; 2], delta: [f32; 2]) -> UiScrolled {
        // Over an open Dropdown's list, the wheel scrolls the list.
        if let Some(list) = &mut self.list
            && list.rect.contains(pointer[0], pointer[1])
        {
            list.scroll = (list.scroll + delta[1] / list.per_unit).clamp(0.0, list.max_scroll);
            self.dropdown = Some((list.id, Some(list.scroll)));
            return UiScrolled::taken();
        }
        let scrollers = self.scrollers_at(world, pointer);
        for element in &scrollers {
            let Some(scroll) = element.scroll else {
                continue;
            };
            let delta = if !scroll.scrolls[1] && scroll.scrolls[0] && delta[0] == 0.0 {
                [delta[1], 0.0]
            } else {
                delta
            };
            let per_unit = element.pixels_per_unit.max(1e-4);
            let to = [
                scroll.position[0] + delta[0] / per_unit,
                scroll.position[1] + delta[1] / per_unit,
            ];
            if let Some(moved) = self.scroll_to(element.id, to) {
                return UiScrolled {
                    taken: true,
                    moved: Some(moved),
                    value: None,
                };
            }
        }
        UiScrolled {
            taken: !scrollers.is_empty(),
            ..UiScrolled::default()
        }
    }

    /// A press at `pointer` on the scroll bar of one of `scrollers` (see
    /// [`Self::scrollers_at`]): that bar is dragged from now on.
    fn press_bars(&mut self, scrollers: &[UiElement], pointer: [f32; 2]) -> Option<UiScrolled> {
        for element in scrollers {
            let Some(scroll) = element.scroll else {
                continue;
            };
            // The bars are where the frame is laid out, before it's turned.
            let local = element.local(pointer);
            for bar in scroll_bars(element.rect, &scroll, element.pixels_per_unit) {
                if !bar.track.contains(local[0], local[1]) {
                    continue;
                }
                let along = local[bar.axis];
                let (grab, moved) = if bar.thumb.contains(local[0], local[1]) {
                    (along - bar.thumb_start(), None)
                } else {
                    let grab = bar.thumb_length() * 0.5;
                    let mut to = scroll.position;
                    to[bar.axis] = bar.position_at(&scroll, along - grab);
                    (grab, self.scroll_to(element.id, to))
                };
                self.scroll_drag = Some(ScrollDrag::Bar {
                    id: element.id,
                    axis: bar.axis,
                    grab,
                });
                return Some(UiScrolled {
                    taken: true,
                    moved,
                    value: None,
                });
            }
        }
        None
    }

    /// [`Self::press`] for scroll bars only, as the UI Designer's artboard
    /// uses it: a press on a ScrollingFrame's bar drags it, and nothing
    /// else is taken (widgets stay still, and content doesn't scroll).
    pub fn press_scroll_bar(&mut self, world: &World, pointer: [f32; 2]) -> UiScrolled {
        self.scroll_drag = None;
        self.widget_press = None;
        let scrollers = self.scrollers_at(world, pointer);
        self.press_bars(&scrollers, pointer).unwrap_or_default()
    }

    /// The left button went down at `pointer`. While a Dropdown's list is
    /// open, the press is taken: on an option, letting go there picks it
    /// (see [`Self::release`]), and anywhere else closes the list. On a
    /// ScrollingFrame's scroll bar, that bar is dragged from now on (a
    /// press on its track first moves the thumb there) and the press is
    /// taken: it presses no button. On an `Interactable` Slider, its thumb
    /// jumps to the pointer and follows it until the button goes up; on a
    /// Toggle or Dropdown, letting go over it flips it or opens its list.
    /// Both are taken. On a frame's content, a drag that moves far enough
    /// scrolls it (see [`Self::drag`]).
    pub fn press(&mut self, world: &World, pointer: [f32; 2]) -> UiScrolled {
        self.scroll_drag = None;
        self.widget_press = None;
        // The mouse takes over from the keyboard or controller.
        self.selected = None;
        self.list_cursor = None;
        if let Some(list) = &self.list {
            if list.rect.contains(pointer[0], pointer[1]) {
                self.widget_press = list
                    .option_at(pointer)
                    .map(|index| WidgetPress::Pick(list.id, index));
            } else {
                // A click outside closes it, and does nothing else.
                self.close_list();
            }
            return UiScrolled::taken();
        }
        let scrollers = self.scrollers_at(world, pointer);
        if let Some(bar) = self.press_bars(&scrollers, pointer) {
            return bar;
        }
        if let Some(top) = self.top_at(pointer).filter(|top| top.interactable)
            && let Some(class) = world.get(top.id).map(|instance| instance.class())
        {
            let id = top.id;
            match class {
                ClassName::Slider => {
                    self.scroll_drag = Some(ScrollDrag::Slider { id });
                    return UiScrolled {
                        value: self.slide(world, id, pointer),
                        ..UiScrolled::taken()
                    };
                }
                ClassName::Toggle => {
                    self.widget_press = Some(WidgetPress::Flip(id));
                    return UiScrolled::taken();
                }
                ClassName::Dropdown => {
                    self.widget_press = Some(WidgetPress::Open(id));
                    return UiScrolled::taken();
                }
                _ => {}
            }
        }
        if let Some(element) = scrollers
            .iter()
            .find(|element| element.scroll.is_some_and(|s| s.scrolls[0] || s.scrolls[1]))
        {
            self.scroll_drag = Some(ScrollDrag::Content {
                id: element.id,
                from: pointer,
                start: element.scroll.map_or([0.0; 2], |scroll| scroll.position),
                moving: false,
            });
        }
        UiScrolled::default()
    }

    /// The pointer is at `pointer` with the left button still held:
    /// carries on a scroll bar's or a Slider's drag, or the content's once
    /// it has moved [`SCROLL_DRAG_SLOP`]. The press is taken while it's
    /// scrolling, so the host lets go of the button it went down on (it
    /// won't fire).
    pub fn drag(&mut self, world: &World, pointer: [f32; 2]) -> UiScrolled {
        match self.scroll_drag {
            Some(ScrollDrag::Slider { id }) => UiScrolled {
                value: self.slide(world, id, pointer),
                ..UiScrolled::taken()
            },
            Some(ScrollDrag::Bar { id, axis, grab }) => {
                let moved = self.element(id).copied().and_then(|element| {
                    let scroll = element.scroll?;
                    let bars = scroll_bars(element.rect, &scroll, element.pixels_per_unit);
                    let bar = bars.iter().find(|bar| bar.axis == axis)?;
                    let mut to = scroll.position;
                    to[axis] = bar.position_at(&scroll, element.local(pointer)[axis] - grab);
                    self.scroll_to(id, to)
                });
                UiScrolled {
                    taken: true,
                    moved,
                    value: None,
                }
            }
            Some(ScrollDrag::Content {
                id,
                from,
                start,
                moving,
            }) => {
                // Measured along the frame's own axes, in case it's turned.
                let (here, there) = self.element(id).map_or((pointer, from), |element| {
                    (element.local(pointer), element.local(from))
                });
                let moved = [here[0] - there[0], here[1] - there[1]];
                let moving = moving || moved[0].hypot(moved[1]) >= SCROLL_DRAG_SLOP;
                self.scroll_drag = Some(ScrollDrag::Content {
                    id,
                    from,
                    start,
                    moving,
                });
                if !moving {
                    return UiScrolled::default();
                }
                let per_unit = self
                    .element(id)
                    .map_or(1.0, |element| element.pixels_per_unit.max(1e-4));
                let to = [
                    start[0] - moved[0] / per_unit,
                    start[1] - moved[1] / per_unit,
                ];
                UiScrolled {
                    taken: true,
                    moved: self.scroll_to(id, to),
                    value: None,
                }
            }
            None => UiScrolled::default(),
        }
    }

    /// The left button went up with the pointer at `pointer` (`None` when
    /// it's off the screen): any scroll or Slider drag ends. Let go over the
    /// Toggle or Dropdown it went down on, it flips the Toggle or opens the
    /// Dropdown's list; over the list option it went down on, it picks
    /// that option and closes the list.
    pub fn release(&mut self, world: &World, pointer: Option<[f32; 2]>) -> UiScrolled {
        self.scroll_drag = None;
        let Some(press) = self.widget_press.take() else {
            return UiScrolled::default();
        };
        let still_on = |id| {
            pointer
                .and_then(|pointer| self.top_at(pointer))
                .is_some_and(|top| top.id == id)
        };
        match press {
            WidgetPress::Flip(id) if still_on(id) => {
                let on = world
                    .get(id)
                    .and_then(|toggle| toggle.property("Value"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                UiScrolled {
                    value: Some((id, Value::Bool(!on))),
                    ..UiScrolled::taken()
                }
            }
            WidgetPress::Open(id) if still_on(id) => {
                self.dropdown = Some((id, None));
                UiScrolled::taken()
            }
            WidgetPress::Pick(id, index) => {
                let picked = self.list.as_ref().and_then(|list| {
                    (pointer.and_then(|pointer| list.option_at(pointer)) == Some(index))
                        .then(|| list.options[index].clone())
                });
                let Some(option) = picked else {
                    return UiScrolled::taken();
                };
                self.close_list();
                let current = world
                    .get(id)
                    .and_then(|dropdown| dropdown.property("Value"))
                    .and_then(Value::as_str);
                UiScrolled {
                    value: (current != Some(option.as_str()))
                        .then_some((id, Value::String(option))),
                    ..UiScrolled::taken()
                }
            }
            _ => UiScrolled::default(),
        }
    }

    /// Whether a scroll bar or a Slider is being dragged, or content
    /// dragged far enough to scroll.
    pub fn scrolling(&self) -> bool {
        matches!(
            self.scroll_drag,
            Some(
                ScrollDrag::Bar { .. }
                    | ScrollDrag::Slider { .. }
                    | ScrollDrag::Content { moving: true, .. }
            )
        )
    }

    /// A click activated `button`: queues its `ClickSound`. Returns whether
    /// it fires `Activated`: an `Interactable` TextButton does.
    pub fn activate(&mut self, world: &World, button: InstanceId) -> bool {
        let fires = world.get(button).is_some_and(|instance| {
            instance.class() == ClassName::TextButton && interactable(instance)
        });
        if fires && let Some(sound) = button_sound(world, button, "ClickSound") {
            self.events.push(UiEvent::PlaySound(sound));
        }
        fires
    }

    /// What happened since the last call, in order.
    pub fn take_events(&mut self) -> Vec<UiEvent> {
        std::mem::take(&mut self.events)
    }
}

/// [`UiInteraction`]'s open Dropdown and how far it's scrolled, the list as
/// last drawn, and the row the keyboard or controller is on.
type OpenList<'a> = (
    &'a mut Option<(InstanceId, Option<f32>)>,
    &'a mut Option<DropdownList>,
    Option<usize>,
);

/// Draws the open Dropdown's list over everything in `layout`, and
/// remembers where it is. It closes when its Dropdown is gone, hidden,
/// not `Interactable` or has no options.
fn draw_list(
    world: &World,
    (width, height): (u32, u32),
    stylesheets: &[Stylesheet],
    pointer: Option<[f32; 2]>,
    (dropdown, list, cursor): OpenList<'_>,
    layout: &mut UiLayout,
) {
    *list = None;
    let Some((id, scroll)) = *dropdown else {
        return;
    };
    let screen = (width as f32, height as f32);
    let drawn = layout
        .element(id)
        .filter(|element| element.interactable)
        .and_then(|element| {
            widget::dropdown_list(world, element, stylesheets, screen, scroll, pointer, cursor)
        });
    match drawn {
        Some((drawn, mut items)) => {
            crate::accessibility::adjust_items(world.ui_settings(), &mut items);
            layout.draw_items.extend(items);
            *dropdown = Some((id, Some(drawn.scroll)));
            *list = Some(drawn);
        }
        None => *dropdown = None,
    }
}

/// Whether an element takes the pointer: its `Interactable` isn't off.
pub(crate) fn interactable(instance: &axcenity_core::Instance) -> bool {
    instance.property("Interactable").and_then(Value::as_bool) != Some(false)
}

/// A TextButton's `HoverSound` or `ClickSound`, if it has one.
fn button_sound(world: &World, id: InstanceId, property: &str) -> Option<String> {
    let instance = world.get(id)?;
    if instance.class() != ClassName::TextButton {
        return None;
    }
    let path = instance.property(property).and_then(Value::as_str)?.trim();
    (!path.is_empty()).then(|| path.to_owned())
}

fn tooltip_text(world: &World, id: InstanceId) -> Option<String> {
    let tip = world.get(id)?.property("Tooltip").and_then(Value::as_str)?;
    (!tip.trim().is_empty()).then(|| tip.to_owned())
}

/// The cursor over `id`: its CSS `cursor`, else a hand on a button and an
/// I-beam on a text box that can be used.
fn cursor(
    world: &World,
    id: InstanceId,
    stylesheets: &[Stylesheet],
    pseudo: PseudoClasses<'_>,
) -> UiCursor {
    let Some(instance) = world.get(id) else {
        return UiCursor::Default;
    };
    let styles = resolve_styles(world, id, stylesheets, pseudo);
    match css_keyword(&styles, "cursor").as_deref() {
        Some("pointer") => UiCursor::Pointer,
        Some("text") => UiCursor::Text,
        Some("not-allowed") => UiCursor::NotAllowed,
        Some(_) => UiCursor::Default,
        None if !interactable(instance) => UiCursor::Default,
        None => match instance.class() {
            ClassName::TextButton | ClassName::Slider | ClassName::Toggle | ClassName::Dropdown => {
                UiCursor::Pointer
            }
            ClassName::TextBox => UiCursor::Text,
            _ => UiCursor::Default,
        },
    }
}

/// The tooltip box for `owner`, saying `tip`, just below and right of the
/// pointer and kept on screen. Styled by `Tooltip` rules; the rest is a
/// small dark box with white text. Its size is worked out from the text's
/// length, wrapping past 320 pixels.
fn tooltip_items(
    world: &World,
    owner: InstanceId,
    stylesheets: &[Stylesheet],
    tip: &str,
    pointer: [f32; 2],
    screen: (f32, f32),
) -> Vec<UiDrawItem> {
    const MAX_WIDTH: f32 = 320.0;
    let styles = resolve_tooltip_styles(world, owner, stylesheets);
    let opacity = css_number(&styles, "opacity")
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    // The box grows with the game UI setting's text scale.
    let font_size = css_length_property(&styles, "font-size")
        .unwrap_or(14.0)
        .max(1.0)
        * world.ui_settings().text_scale as f32;
    let padding = css_box_values(&styles, "padding").map_or([6.0, 8.0, 6.0, 8.0], |sides| {
        sides.map(|side| match side {
            taffy::LengthPercentage::Length(pixels) => pixels.max(0.0),
            taffy::LengthPercentage::Percent(_) => 0.0,
        })
    });
    let longest = tip
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0) as f32;
    let text_width = (longest * font_size * 0.56).clamp(font_size, MAX_WIDTH);
    let lines: f32 = tip
        .lines()
        .map(|line| {
            (line.chars().count() as f32 * font_size * 0.56 / MAX_WIDTH)
                .ceil()
                .max(1.0)
        })
        .sum::<f32>()
        .max(1.0);
    let text_height = lines * font_size * 1.25;
    let size = [
        text_width + padding[1] + padding[3],
        text_height + padding[0] + padding[2],
    ];
    let mut position = [pointer[0] + 14.0, pointer[1] + 20.0];
    if position[0] + size[0] > screen.0 - 4.0 {
        position[0] = (screen.0 - 4.0 - size[0]).max(0.0);
    }
    if position[1] + size[1] > screen.1 - 4.0 {
        position[1] = (pointer[1] - 8.0 - size[1]).max(0.0);
    }

    let (background, background_alpha) = css_color(&styles, "background-color")
        .unwrap_or((axcenity_core::Color3::new(0.09, 0.09, 0.11), 0.94));
    let (text_color, text_alpha) =
        css_color(&styles, "color").unwrap_or((axcenity_core::Color3::new(1.0, 1.0, 1.0), 1.0));
    let radius = css_length_property(&styles, "border-radius")
        .unwrap_or(4.0)
        .max(0.0);
    let mut items = Vec::new();
    let fill = [
        background.r,
        background.g,
        background.b,
        background_alpha * opacity,
    ];
    crate::paint::paint_box(
        &crate::paint::BoxPaint {
            rect: crate::UiRect {
                x: position[0],
                y: position[1],
                width: size[0],
                height: size[1],
            },
            fill,
            fill_end: fill,
            radius,
            opacity,
            focus_ring: None,
            border: true,
        },
        &styles,
        &mut items,
    );
    items.push(UiDrawItem::Text(UiText {
        position: [position[0] + padding[3], position[1] + padding[0]],
        size: [text_width, text_height],
        text: tip.to_owned(),
        font_size,
        font_family: css_property(&styles, "font-family")
            .map(|family| crate::font_family_name(&family))
            .unwrap_or_else(|| "Montserrat".to_owned()),
        color: [
            text_color.r,
            text_color.g,
            text_color.b,
            text_alpha * opacity,
        ],
        weight: css_font_weight(&styles),
        wrap: true,
        ..UiText::default()
    }));
    items
}
