//! The pointer and the UI over time: hover and press states, CSS
//! transitions, `MouseEnter` and `MouseLeave`, tooltips, the cursor,
//! buttons' hover and click sounds, and scrolling ScrollingFrames. Hosts
//! (the player, the editor's viewport) keep one [`UiInteraction`] for a
//! game and lay their UI out through it each frame.

use std::cell::RefCell;

use axcenity_core::{ClassName, InstanceId, Value, Vector2, World};
use axcenity_render::{Camera, UiDrawItem, UiText};

use crate::css::*;
use crate::style::{PseudoClasses, Stylesheet, resolve_styles, resolve_tooltip_styles};
use crate::transition::Transitions;
use crate::{UiElement, UiLayout, layout_ui_with_pseudo, scroll_bars};

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
    /// Where each element was in the last layout, for scrolling.
    elements: Vec<UiElement>,
    scroll_drag: Option<ScrollDrag>,
}

/// What the pointer did to the ScrollingFrames: [`UiInteraction::wheel`],
/// [`UiInteraction::press`] and [`UiInteraction::drag`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[must_use = "a scroll only shows once its move is applied to the world"]
pub struct UiScrolled {
    /// The scroll took the wheel or the press, so the host doesn't use it
    /// for anything else: the camera doesn't zoom, and no button is pressed.
    pub taken: bool,
    /// The ScrollingFrame to set `CanvasPosition` on, and what to.
    pub moved: Option<(InstanceId, Vector2)>,
}

impl UiScrolled {
    /// Sets the moved frame's `CanvasPosition` in `world`. Returns [`Self::taken`].
    pub fn apply(self, world: &mut World) -> bool {
        if let Some((id, at)) = self.moved {
            // A frame removed since the last layout has nothing to set.
            let _ = world.set_property(id, "CanvasPosition", Value::Vector2(at));
        }
        self.taken
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
        let base = layout_ui_with_pseudo(
            world,
            width,
            height,
            stylesheets,
            PseudoClasses {
                focused: input.focused,
                ..PseudoClasses::default()
            },
            camera,
            input.on_top,
        );
        let under = input.pointer.and_then(|[x, y]| base.hit(x, y));
        let hovered = input.pointer.and_then(|[x, y]| base.hover_target(x, y));
        let active = input.pressed.filter(|button| hovered == Some(*button));
        let pseudo = PseudoClasses {
            hovered,
            active,
            focused: input.focused,
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
            input.on_top,
        );
        layout.hovered = hovered;
        layout.active = active;

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
                layout.draw_items.extend(tooltip_items(
                    world,
                    id,
                    stylesheets,
                    &tip,
                    pointer,
                    (width as f32, height as f32),
                ));
            } else {
                // Nothing moves while the pointer rests, so keep drawing
                // until the tooltip is due.
                layout.animating = true;
            }
        }
        layout.cursor = under.map_or(UiCursor::Default, |id| {
            cursor(world, id, stylesheets, pseudo)
        });
        layout.animating |= self.transitions.borrow().running();
        self.elements.clone_from(&layout.elements);
        layout
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
                };
            }
        }
        UiScrolled {
            taken: !scrollers.is_empty(),
            moved: None,
        }
    }

    /// The left button went down at `pointer`. On a ScrollingFrame's scroll
    /// bar, that bar is dragged from now on (a press on its track first
    /// moves the thumb there) and the press is taken: it presses no
    /// button. On a frame's content, a drag that moves far enough scrolls
    /// it (see [`Self::drag`]).
    pub fn press(&mut self, world: &World, pointer: [f32; 2]) -> UiScrolled {
        self.scroll_drag = None;
        let scrollers = self.scrollers_at(world, pointer);
        for element in &scrollers {
            let Some(scroll) = element.scroll else {
                continue;
            };
            for bar in scroll_bars(element.rect, &scroll, element.pixels_per_unit) {
                if !bar.track.contains(pointer[0], pointer[1]) {
                    continue;
                }
                let along = pointer[bar.axis];
                let (grab, moved) = if bar.thumb.contains(pointer[0], pointer[1]) {
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
                return UiScrolled { taken: true, moved };
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
    /// carries on a scroll bar's drag, or the content's once it has moved
    /// [`SCROLL_DRAG_SLOP`]. The press is taken while it's scrolling, so
    /// the host lets go of the button it went down on (it won't fire).
    pub fn drag(&mut self, pointer: [f32; 2]) -> UiScrolled {
        match self.scroll_drag {
            Some(ScrollDrag::Bar { id, axis, grab }) => {
                let moved = self.element(id).copied().and_then(|element| {
                    let scroll = element.scroll?;
                    let bars = scroll_bars(element.rect, &scroll, element.pixels_per_unit);
                    let bar = bars.iter().find(|bar| bar.axis == axis)?;
                    let mut to = scroll.position;
                    to[axis] = bar.position_at(&scroll, pointer[axis] - grab);
                    self.scroll_to(id, to)
                });
                UiScrolled { taken: true, moved }
            }
            Some(ScrollDrag::Content {
                id,
                from,
                start,
                moving,
            }) => {
                let moved = [pointer[0] - from[0], pointer[1] - from[1]];
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
                }
            }
            None => UiScrolled::default(),
        }
    }

    /// The left button went up: any scroll drag ends.
    pub fn release(&mut self) {
        self.scroll_drag = None;
    }

    /// Whether a scroll bar is being dragged, or content dragged far
    /// enough to scroll.
    pub fn scrolling(&self) -> bool {
        matches!(
            self.scroll_drag,
            Some(ScrollDrag::Bar { .. } | ScrollDrag::Content { moving: true, .. })
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
            ClassName::TextButton => UiCursor::Pointer,
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
    let font_size = css_length_property(&styles, "font-size")
        .unwrap_or(14.0)
        .max(1.0);
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
