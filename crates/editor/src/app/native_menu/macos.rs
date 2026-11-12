//! The macOS menu bar, through `muda` (the menu crate Tauri uses): turns
//! the menus of `native_menu.rs` into an `NSMenu` for the app, keeps it in
//! step each frame and hands back what was chosen.
//!
//! An item with a shortcut gets it as its key equivalent, so the menu shows
//! the keys and flashes its title when they're pressed. macOS then gives
//! those keys to the menu instead of the window, so when keys (not a click)
//! chose an item, the same keys are handed to egui and go wherever they go
//! on Windows and Linux: Cmd+Z in a text field still undoes the typing.
//! For the same reason such an item stays enabled even when its command
//! can't run now, as a disabled item would swallow its keys.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui::{self, Key, KeyboardShortcut};
use muda::accelerator::{Key as MenuKey, KeyAccelerator, Modifiers, NamedKey};
use muda::{CheckMenuItem, Menu, MenuEvent, MenuId, PredefinedMenuItem, Submenu};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSEventType};

use super::{APP_NAME, MenuAction, MenuItem, MenuNode, MenuRole, SystemItem, TopMenu};
use crate::app::command_palette::Command;
use crate::shortcuts;

/// An item chosen from the menu bar.
struct Chosen {
    id: MenuId,
    /// By its keys rather than a click.
    by_keys: bool,
}

/// What an item id does, and the keys it has in the menu.
struct Target {
    action: MenuAction,
    keys: Option<KeyboardShortcut>,
}

/// The live menu bar. It lives on the main thread, where AppKit wants it
/// (`muda`'s menus aren't `Send`), not in `EditorApp`.
struct Live {
    /// Kept so the menu bar stays the app's.
    #[allow(dead_code)]
    menu: Menu,
    tops: Vec<(Submenu, Vec<Handle>)>,
    shown: Vec<TopMenu>,
}

/// One built item, to change in place.
enum Handle {
    Item(muda::MenuItem),
    Check(CheckMenuItem),
    Heading(muda::MenuItem),
    Submenu(Submenu, Vec<Handle>),
    /// Separators and the system's items, which never change.
    Fixed(PredefinedMenuItem),
}

thread_local! {
    static LIVE: RefCell<Option<Live>> = const { RefCell::new(None) };
}

/// The menu bar's side in `EditorApp`: what each item does, and the items
/// chosen since the last frame.
pub(in crate::app) struct Bar {
    targets: HashMap<MenuId, Target>,
    chosen: Receiver<Chosen>,
}

impl Bar {
    /// Starts listening for chosen items. The menu bar itself is built on
    /// the first [`Bar::sync`].
    pub(in crate::app) fn new(ctx: &egui::Context) -> Self {
        let (sender, chosen) = channel();
        let ctx = ctx.clone();
        // Runs on the main thread, inside AppKit's handling of the click or
        // the keys, so the event that chose the item is still current.
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let _ = sender.send(Chosen {
                id: event.id,
                by_keys: chosen_by_keys(),
            });
            // egui only draws when something happens, and a menu isn't
            // one of its events.
            ctx.request_repaint();
        }));
        Self {
            targets: HashMap::new(),
            chosen,
        }
    }

    /// Makes the menu bar show `menus`: changes items in place where the
    /// menus kept their shape, and rebuilds a menu whose items changed.
    pub(in crate::app) fn sync(&mut self, menus: &[TopMenu]) {
        LIVE.with_borrow_mut(|live| {
            let same_menus = live.as_ref().is_some_and(|live| {
                live.shown.len() == menus.len()
                    && live
                        .shown
                        .iter()
                        .zip(menus)
                        .all(|(a, b)| a.title == b.title && a.role == b.role)
            });
            let Some(live) = live.as_mut().filter(|_| same_menus) else {
                *live = Some(Live::build(menus));
                return;
            };
            if live.shown == menus {
                return;
            }
            for (index, menu) in menus.iter().enumerate() {
                let (submenu, handles) = &mut live.tops[index];
                if same_shape(&live.shown[index].items, &menu.items) {
                    update(handles, &menu.items);
                } else {
                    while submenu.remove_at(0).is_some() {}
                    *handles = build(submenu, &menu.items, &index.to_string());
                }
            }
            live.shown = menus.to_vec();
        });
        self.targets.clear();
        for (index, menu) in menus.iter().enumerate() {
            collect(&mut self.targets, &menu.items, &index.to_string());
        }
    }

    /// The items chosen since the last frame. Those chosen by their keys
    /// put the keys into `raw_input` instead, for egui to handle as usual.
    pub(in crate::app) fn take_chosen(
        &mut self,
        raw_input: &mut egui::RawInput,
    ) -> Vec<MenuAction> {
        let mut clicked = Vec::new();
        while let Ok(chosen) = self.chosen.try_recv() {
            let Some(target) = self.targets.get(&chosen.id) else {
                continue;
            };
            match (chosen.by_keys, target.keys) {
                (true, Some(keys)) => {
                    let mut modifiers = keys.modifiers;
                    // Cmd on a Mac is both, as egui-winit reports it.
                    modifiers.mac_cmd |= modifiers.command;
                    raw_input.events.push(egui::Event::Key {
                        key: keys.logical_key,
                        physical_key: Some(keys.logical_key),
                        pressed: true,
                        repeat: false,
                        modifiers,
                    });
                }
                _ => clicked.push(target.action.clone()),
            }
        }
        clicked
    }
}

impl Live {
    fn build(menus: &[TopMenu]) -> Self {
        let menu = Menu::new();
        let mut tops = Vec::new();
        for (index, top) in menus.iter().enumerate() {
            let submenu = Submenu::with_id(format!("top {index}"), top.title, true);
            let handles = build(&submenu, &top.items, &index.to_string());
            if let Err(error) = menu.append(&submenu) {
                log::warn!("menu bar: couldn't add {}: {error}", top.title);
            }
            tops.push((submenu, handles));
        }
        menu.init_for_nsapp();
        // After the menu is the app's, so macOS can find these in it.
        for (top, (submenu, _)) in menus.iter().zip(&tops) {
            match top.role {
                MenuRole::Window => submenu.set_as_windows_menu_for_nsapp(),
                MenuRole::Help => submenu.set_as_help_menu_for_nsapp(),
                MenuRole::App | MenuRole::Plain => {}
            }
        }
        Self {
            menu,
            tops,
            shown: menus.to_vec(),
        }
    }
}

/// Builds `items` into `parent`. Each item's id is its path of positions
/// (`"1.12.3"`), so a rebuilt menu keeps the ids of its unchanged items.
fn build(parent: &Submenu, items: &[MenuNode], path: &str) -> Vec<Handle> {
    let mut handles = Vec::new();
    for (index, node) in items.iter().enumerate() {
        let id = format!("{path}.{index}");
        let handle = match node {
            MenuNode::Item(item) => {
                let keys = menu_keys(item);
                let enabled = item.enabled || keys.is_some();
                match item.checked {
                    Some(checked) => Handle::Check(CheckMenuItem::with_id(
                        id,
                        &item.label,
                        enabled,
                        checked,
                        None,
                    )),
                    None => {
                        let built = muda::MenuItem::with_id(id, &item.label, enabled, None);
                        if let Some(keys) = keys
                            && let Err(error) = built.set_key_accelerator(Some(keys))
                        {
                            log::warn!("menu bar: no keys for {}: {error}", item.label);
                        }
                        Handle::Item(built)
                    }
                }
            }
            MenuNode::Submenu { label, items } => {
                let submenu = Submenu::with_id(id.clone(), label, true);
                let children = build(&submenu, items, &id);
                Handle::Submenu(submenu, children)
            }
            MenuNode::Heading(text) => Handle::Heading(muda::MenuItem::new(text, false, None)),
            MenuNode::System(item) => Handle::Fixed(system_item(*item)),
            MenuNode::Separator => Handle::Fixed(PredefinedMenuItem::separator()),
        };
        let appended = match &handle {
            Handle::Item(item) | Handle::Heading(item) => parent.append(item),
            Handle::Check(item) => parent.append(item),
            Handle::Submenu(submenu, _) => parent.append(submenu),
            Handle::Fixed(item) => parent.append(item),
        };
        if let Err(error) = appended {
            log::warn!("menu bar: couldn't add an item: {error}");
        }
        handles.push(handle);
    }
    handles
}

/// Changes the built items to match `items`, which have their shape.
fn update(handles: &mut [Handle], items: &[MenuNode]) {
    for (handle, node) in handles.iter_mut().zip(items) {
        match (handle, node) {
            (Handle::Item(built), MenuNode::Item(item)) => {
                if built.text() != item.label {
                    built.set_text(&item.label);
                }
                let enabled = item.enabled || menu_keys(item).is_some();
                if built.is_enabled() != enabled {
                    built.set_enabled(enabled);
                }
            }
            (Handle::Check(built), MenuNode::Item(item)) => {
                if built.text() != item.label {
                    built.set_text(&item.label);
                }
                if built.is_enabled() != item.enabled {
                    built.set_enabled(item.enabled);
                }
                let checked = item.checked.unwrap_or(false);
                if built.is_checked() != checked {
                    built.set_checked(checked);
                }
            }
            (Handle::Heading(built), MenuNode::Heading(text)) => {
                if built.text() != *text {
                    built.set_text(text);
                }
            }
            (Handle::Submenu(built, children), MenuNode::Submenu { label, items }) => {
                if built.text() != *label {
                    built.set_text(label);
                }
                update(children, items);
            }
            _ => {}
        }
    }
}

/// Whether `a` and `b` can share built items: the same kinds of item in
/// the same places, with the same keys.
fn same_shape(a: &[MenuNode], b: &[MenuNode]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|pair| match pair {
            (MenuNode::Item(a), MenuNode::Item(b)) => {
                a.checked.is_some() == b.checked.is_some() && a.shortcut == b.shortcut
            }
            (MenuNode::Submenu { items: a, .. }, MenuNode::Submenu { items: b, .. }) => {
                same_shape(a, b)
            }
            (MenuNode::Heading(_), MenuNode::Heading(_))
            | (MenuNode::Separator, MenuNode::Separator) => true,
            (MenuNode::System(a), MenuNode::System(b)) => a == b,
            _ => false,
        })
}

/// Notes what each item id does, with the ids `build` gives them.
fn collect(targets: &mut HashMap<MenuId, Target>, items: &[MenuNode], path: &str) {
    for (index, node) in items.iter().enumerate() {
        let id = format!("{path}.{index}");
        match node {
            MenuNode::Item(item) => {
                let keys = item.shortcut.and_then(shortcut_keys);
                targets.insert(
                    MenuId::new(&id),
                    Target {
                        action: item.action.clone(),
                        keys,
                    },
                );
            }
            MenuNode::Submenu { items, .. } => collect(targets, items, &id),
            MenuNode::Heading(_) | MenuNode::System(_) | MenuNode::Separator => {}
        }
    }
}

fn system_item(item: SystemItem) -> PredefinedMenuItem {
    match item {
        SystemItem::Services => PredefinedMenuItem::services(None),
        SystemItem::Hide => PredefinedMenuItem::hide(Some(&format!("Hide {APP_NAME}"))),
        SystemItem::HideOthers => PredefinedMenuItem::hide_others(None),
        SystemItem::ShowAll => PredefinedMenuItem::show_all(None),
        SystemItem::Minimize => PredefinedMenuItem::minimize(None),
        SystemItem::Zoom => PredefinedMenuItem::zoom(None),
        // macOS renames it Exit Full Screen while the window is.
        SystemItem::FullScreen => PredefinedMenuItem::fullscreen(Some("Enter Full Screen")),
        SystemItem::BringAllToFront => PredefinedMenuItem::bring_all_to_front(None),
    }
}

/// The keys an item has in the menu: Cmd+Q for Quit, as on every Mac,
/// and otherwise its first keys from the shortcut table (Shift+Cmd+Z for
/// Redo, the Mac's own), when they have a modifier or are a function key.
/// A plain key such as Backspace is left off, as the menu would take it
/// from every text field.
fn menu_keys(item: &MenuItem) -> Option<KeyAccelerator> {
    if item.action == MenuAction::Command(Command::Quit) {
        return Some(KeyAccelerator::new(
            Modifiers::META,
            MenuKey::Character("q".to_owned()),
        ));
    }
    let keys = shortcut_keys(item.shortcut?)?;
    let mut modifiers = Modifiers::empty();
    if keys.modifiers.command || keys.modifiers.mac_cmd {
        modifiers |= Modifiers::META;
    }
    if keys.modifiers.ctrl {
        modifiers |= Modifiers::CONTROL;
    }
    if keys.modifiers.alt {
        modifiers |= Modifiers::ALT;
    }
    if keys.modifiers.shift {
        modifiers |= Modifiers::SHIFT;
    }
    let key = match function_key(keys.logical_key) {
        Some(named) => MenuKey::Named(named),
        None => {
            let symbol = keys.logical_key.symbol_or_name();
            if symbol.chars().count() != 1 {
                return None;
            }
            MenuKey::Character(symbol.to_lowercase())
        }
    };
    Some(KeyAccelerator::new(modifiers, key))
}

/// The keys from the shortcut table an item takes into the menu (see
/// [`menu_keys`]), or `None` to leave them to the window.
fn shortcut_keys(action: shortcuts::Action) -> Option<KeyboardShortcut> {
    let keys = action.keys();
    // The Mac's own Redo, where the table has it too.
    let chord = keys
        .iter()
        .find(|chord| {
            action == shortcuts::Action::Redo
                && chord.logical_key == Key::Z
                && chord.modifiers.shift
        })
        .or_else(|| keys.first())?;
    let modified = chord.modifiers.command
        || chord.modifiers.mac_cmd
        || chord.modifiers.ctrl
        || chord.modifiers.alt;
    (modified || function_key(chord.logical_key).is_some()).then_some(*chord)
}

fn function_key(key: Key) -> Option<NamedKey> {
    Some(match key {
        Key::F1 => NamedKey::F1,
        Key::F2 => NamedKey::F2,
        Key::F3 => NamedKey::F3,
        Key::F4 => NamedKey::F4,
        Key::F5 => NamedKey::F5,
        Key::F6 => NamedKey::F6,
        Key::F7 => NamedKey::F7,
        Key::F8 => NamedKey::F8,
        Key::F9 => NamedKey::F9,
        Key::F10 => NamedKey::F10,
        Key::F11 => NamedKey::F11,
        Key::F12 => NamedKey::F12,
        _ => return None,
    })
}

/// Whether the event AppKit is handling now is a key press: an item chosen
/// by its key equivalent rather than a click.
fn chosen_by_keys() -> bool {
    let Some(main_thread) = MainThreadMarker::new() else {
        return false;
    };
    NSApplication::sharedApplication(main_thread)
        .currentEvent()
        .is_some_and(|event| event.r#type() == NSEventType::KeyDown)
}
