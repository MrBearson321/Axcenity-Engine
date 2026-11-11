//! Running the editor's Lua plugins (Phase 54; `docs/editor/plugins.md`):
//! starting and reloading them, calling them (a ribbon button, a new
//! selection, the Scene View's mouse, their dock tabs), and turning what
//! they did to the world into undo steps.
//!
//! Plugins only run while editing: during Play their buttons are off and
//! nothing calls them. Each call runs on the edit world with its change
//! log on. Afterwards the Instances the log names become one undo step
//! (`History::record_edits`), which later calls add to until the plugin
//! sets a waypoint. Their output goes to the Console; their errors go there
//! too and to the Plugins panel, and the editor carries on.

use axcenity_plugins::{MouseInput, Plugin, Request, Start};
use axcenity_ui::UiInput;

use super::*;
use crate::history::changed_ids;
use crate::plugins::{Entry, PluginAction, WidgetView};

/// A ribbon button's icon: a Phosphor icon by name (`"grid-four"`), or the
/// text itself when it's one symbol. Anything else is a puzzle piece.
fn button_icon(name: &str) -> &str {
    let known = match name {
        "grid-four" => Some(icons::GRID_FOUR),
        "magnet" => Some(icons::MAGNET),
        "cube" => Some(icons::CUBE),
        "ruler" => Some(icons::RULER),
        "paint-brush" => Some(icons::PAINT_BRUSH),
        "palette" => Some(icons::PALETTE),
        "wrench" => Some(icons::WRENCH),
        "hammer" => Some(icons::HAMMER),
        "magic-wand" => Some(icons::MAGIC_WAND),
        "sparkle" => Some(icons::SPARKLE),
        "star" => Some(icons::STAR),
        "lightning" => Some(icons::LIGHTNING),
        "tree" => Some(icons::TREE),
        "mountains" => Some(icons::MOUNTAINS),
        "eye" => Some(icons::EYE),
        "copy" => Some(icons::COPY),
        "trash" => Some(icons::TRASH),
        "plus" => Some(icons::PLUS),
        "arrows-out-cardinal" => Some(icons::ARROWS_OUT_CARDINAL),
        "align-bottom" => Some(icons::ALIGN_BOTTOM),
        "align-left" => Some(icons::ALIGN_LEFT),
        "shuffle" => Some(icons::SHUFFLE),
        _ => None,
    };
    match known {
        Some(icon) => icon,
        None if name.chars().count() == 1 => name,
        None => icons::PUZZLE_PIECE,
    }
}

impl EditorApp {
    /// Where the plugins' saved state and the user's plugins are (the
    /// editor's data folder), and starts the user's plugins if a project
    /// is open.
    pub fn set_plugin_data(&mut self, data: Option<PathBuf>) {
        self.plugins = crate::plugins::Plugins::new(data);
        self.load_plugins();
    }

    /// Stops every plugin, looks for them again, and starts the ones that
    /// are on. Only with a project open: plugins edit its world.
    pub(in crate::app) fn load_plugins(&mut self) {
        for index in 0..self.plugins.entries.len() {
            self.stop_plugin(index);
        }
        self.plugins.entries.clear();
        self.plugins.watchers.clear();
        self.plugins.open_step = None;
        let Some(project) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        let user = self.plugins.user_folder();
        for found in axcenity_plugins::find_plugins(Some(&project), user.as_deref()) {
            self.plugins.entries.push(Entry {
                found,
                running: None,
                errors: Vec::new(),
                stylesheets: Vec::new(),
                widgets: Vec::new(),
            });
        }
        let roots = [
            (
                axcenity_plugins::Origin::Project,
                Some(project.join(axcenity_plugins::PLUGINS_FOLDER)),
            ),
            (axcenity_plugins::Origin::User, user),
        ];
        for (origin, root) in roots {
            if let Some(root) = root {
                let watcher = axcenity_project::Watcher::start_listing(root, list_plugin_files);
                self.plugins.watchers.push((origin, watcher));
            }
        }
        for index in 0..self.plugins.entries.len() {
            self.start_plugin(index);
        }
    }

    /// Starts the plugin at `index` if it's on, reading its stylesheets.
    fn start_plugin(&mut self, index: usize) {
        let Some(entry) = self.plugins.entries.get_mut(index) else {
            return;
        };
        entry.errors.clear();
        entry.stylesheets = read_stylesheets(&entry.found.folder);
        let (found, shown) = (entry.found.clone(), entry.shown());
        if !self.plugins.is_enabled(&found.key) {
            return;
        }
        let start = Start {
            project: self.project.as_ref().map(|project| project.folder.clone()),
            settings: self.plugins.settings(&found.key),
            selection: self.selection.clone(),
            shown,
        };
        let was_recording = self.world.is_recording_changes();
        self.world.record_changes();
        let leftover = self.take_world_changes();
        drop(leftover);
        let before = self.world.clone();
        let mut started = None;
        if let Err(crash) = play::guard(|| {
            started = Some(Plugin::start(&mut self.world, &found, start));
        }) {
            started = Some(Err(format!("the engine crashed starting it: {}", crash.0)));
        }
        match started.expect("set above") {
            Ok(plugin) => self.plugins.entries[index].running = Some(plugin),
            Err(message) => {
                self.output.error(format!(
                    "Plugin \"{}\" didn't start: {message}",
                    found.name()
                ));
                self.plugins.entries[index].error(message);
            }
        }
        // What starting did to the game is a step too (or the open one).
        self.after_plugin_call(index, before, was_recording);
    }

    /// Stops the plugin at `index`: its dock tabs go.
    fn stop_plugin(&mut self, index: usize) {
        let Some(entry) = self.plugins.entries.get_mut(index) else {
            return;
        };
        entry.widgets.clear();
        if let Some(plugin) = entry.running.take() {
            plugin.stop(&mut self.world);
        }
    }

    /// Stops and starts the plugin at `index`, reading its files again.
    pub(in crate::app) fn reload_plugin(&mut self, index: usize) {
        self.stop_plugin(index);
        if let Some(entry) = self.plugins.entries.get_mut(index)
            && let Ok(text) =
                std::fs::read_to_string(entry.found.folder.join(axcenity_plugins::MANIFEST_FILE))
        {
            entry.found.manifest = axcenity_plugins::Manifest::parse(&text);
        }
        self.start_plugin(index);
    }

    /// Calls the running plugin at `index` with `f`, then records its edits
    /// for undo and does what it asked.
    fn call_plugin(&mut self, index: usize, f: impl FnOnce(&Plugin, &mut World)) {
        if self.playing() {
            return;
        }
        let Some(plugin) = self
            .plugins
            .entries
            .get_mut(index)
            .and_then(|entry| entry.running.take())
        else {
            return;
        };
        let was_recording = self.world.is_recording_changes();
        self.world.record_changes();
        // Changes from before the call are someone else's.
        drop(self.take_world_changes());
        let before = self.world.clone();
        // It hears of a new selection first, if it missed it.
        let selection = self.selection.clone();
        if let Err(crash) = play::guard(|| {
            plugin.set_selection(&mut self.world, &selection);
            f(&plugin, &mut self.world)
        }) {
            let message = format!("the engine crashed running it: {}", crash.0);
            self.output.error(format!(
                "Plugin \"{}\": {message}",
                self.plugins.entries[index].found.name()
            ));
            self.plugins.entries[index].error(message);
        }
        self.plugins.entries[index].running = Some(plugin);
        self.after_plugin_call(index, before, was_recording);
    }

    /// After a call to the plugin at `index`: its edits since `before`
    /// become undo steps, and its requests, output and settings are taken.
    fn after_plugin_call(&mut self, index: usize, before: World, was_recording: bool) {
        let name = self.plugins.entries[index].found.name();
        let requests = match &self.plugins.entries[index].running {
            Some(plugin) => plugin.take_requests(),
            None => Vec::new(),
        };
        let mut segment = before;
        let mut open = self.plugins.open_step;
        let mut select = None;
        let mut history_moves = Vec::new();
        for request in requests {
            match request {
                Request::Waypoint(waypoint) => {
                    if let Some(at) = waypoint.world {
                        let ids = changed_ids(&waypoint.changes);
                        open = self.history.record_edits(
                            &mut self.world,
                            waypoint.name.clone(),
                            (&segment, Some(&at)),
                            &ids,
                            open,
                        );
                        segment = *at;
                    }
                    // A waypoint ends the open step, and names it.
                    if let Some(number) = open.take() {
                        self.history.rename_top(number, waypoint.name);
                    }
                }
                Request::Select(ids) => select = Some(ids),
                Request::Undo => history_moves.push(true),
                Request::Redo => history_moves.push(false),
            }
        }
        let rest = self.take_world_changes();
        let ids = changed_ids(&rest);
        open =
            self.history
                .record_edits(&mut self.world, name.clone(), (&segment, None), &ids, open);
        self.plugins.open_step = open;
        if !was_recording {
            self.world.stop_recording_changes();
        }
        self.forget_selection_outside_tree();
        if let Some(ids) = select {
            self.select_many(ids);
        }
        for undo in history_moves {
            if undo {
                self.undo();
            } else {
                self.redo();
            }
        }
        self.take_plugin_output(index);
    }

    /// Moves the plugin at `index`'s output to the Console and its errors
    /// to the Plugins panel too, and saves its settings if they changed.
    fn take_plugin_output(&mut self, index: usize) {
        let Some(entry) = self.plugins.entries.get_mut(index) else {
            return;
        };
        let Some(plugin) = &entry.running else {
            return;
        };
        let lines = plugin.take_output();
        let settings = plugin.take_settings();
        for line in lines {
            match line.kind {
                axcenity_script::OutputKind::Print => self.output.info(line.text),
                axcenity_script::OutputKind::Warning => self.output.warning(line.text),
                axcenity_script::OutputKind::Error => {
                    entry.error(line.text.clone());
                    self.output.script_error(line.text, line.trace);
                }
            }
        }
        if let Some(settings) = settings {
            let key = entry.found.key.clone();
            self.plugins.set_settings(&key, settings);
        }
        self.sync_plugin_widgets(index);
    }

    /// Keeps the plugin at `index`'s dock tabs in line with the widgets
    /// it made.
    fn sync_plugin_widgets(&mut self, index: usize) {
        let Some(entry) = self.plugins.entries.get(index) else {
            return;
        };
        let widgets = entry
            .running
            .as_ref()
            .map(Plugin::widgets)
            .unwrap_or_default();
        let key = entry.found.key.clone();
        let mut views = std::mem::take(&mut self.plugins.entries[index].widgets);
        views.retain(|view| widgets.iter().any(|w| w.gui == view.widget.gui));
        for widget in widgets {
            if views.iter().any(|view| view.widget.gui == widget.gui) {
                continue;
            }
            let pane = self.plugins.pane_id(&key, &widget.id);
            views.push(WidgetView {
                pane,
                ui: axcenity_ui::UiInteraction::only(widget.gui),
                widget,
                pressed: None,
                canvas: Default::default(),
            });
        }
        self.plugins.entries[index].widgets = views;
    }

    /// The plugins' part of a frame while editing: reload changed plugins,
    /// tell them the selection, the Scene View's mouse and the time, and
    /// keep their dock tabs open.
    pub(in crate::app) fn plugins_frame(&mut self, seconds: f64) {
        if self.playing() || self.project.is_none() {
            return;
        }
        self.reload_changed_plugins();
        // A plugin whose UI went (a Console command's undo swaps the whole
        // world) starts again.
        for index in 0..self.plugins.entries.len() {
            let lost = self.plugins.entries[index]
                .running
                .as_ref()
                .is_some_and(|plugin| plugin.widgets().iter().any(|w| !self.world.contains(w.gui)));
            if lost {
                self.plugins.entries[index].running = None;
                self.reload_plugin(index);
            }
        }
        if self.plugins.told_selection != self.selection {
            self.plugins.told_selection = self.selection.clone();
            let selection = self.selection.clone();
            for index in 0..self.plugins.entries.len() {
                self.call_plugin(index, |plugin, world| {
                    plugin.set_selection(world, &selection)
                });
            }
        }
        for index in 0..self.plugins.entries.len() {
            let waiting = self.plugins.entries[index]
                .running
                .as_ref()
                .is_some_and(Plugin::has_waiting);
            if waiting {
                self.call_plugin(index, |plugin, world| plugin.step(world, seconds));
            }
        }
        let open: Vec<(u64, bool)> = self
            .plugins
            .entries
            .iter()
            .flat_map(|entry| &entry.widgets)
            .map(|view| (view.pane, widget_enabled(&self.world, view.widget.gui)))
            .collect();
        let shown: Vec<u64> = open
            .iter()
            .filter(|(_, enabled)| *enabled)
            .map(|(pane, _)| *pane)
            .collect();
        self.layout.sync_plugins(&shown);
    }

    /// Plugins whose files changed on disk start again; new plugin folders
    /// start, and removed ones stop.
    fn reload_changed_plugins(&mut self) {
        let mut changed: Vec<String> = Vec::new();
        for (origin, watcher) in &self.plugins.watchers {
            let prefix = match origin {
                axcenity_plugins::Origin::Project => "project",
                axcenity_plugins::Origin::User => "user",
            };
            for path in watcher.changes() {
                if let Some(folder) = path.split('/').next() {
                    changed.push(format!("{prefix}/{folder}"));
                }
            }
        }
        if changed.is_empty() {
            return;
        }
        let known: Vec<String> = self
            .plugins
            .entries
            .iter()
            .map(|entry| entry.found.key.clone())
            .collect();
        // A plugin folder came or went: look again for all of them.
        if changed.iter().any(|key| !known.contains(key))
            || self.plugins.entries.iter().any(|entry| {
                !entry
                    .found
                    .folder
                    .join(axcenity_plugins::MANIFEST_FILE)
                    .is_file()
            })
        {
            self.load_plugins();
            return;
        }
        for index in 0..self.plugins.entries.len() {
            if changed.contains(&self.plugins.entries[index].found.key) {
                self.output.info(format!(
                    "Reloaded the plugin \"{}\".",
                    self.plugins.entries[index].found.name()
                ));
                self.reload_plugin(index);
            }
        }
    }

    /// A plugin's toolbar button was clicked in the ribbon.
    pub(in crate::app) fn click_plugin_button(&mut self, index: usize, button: u32) {
        self.call_plugin(index, |plugin, world| plugin.click(world, button));
    }

    /// The Scene View's mouse, for plugins that asked for it.
    pub(in crate::app) fn plugin_mouse(&mut self, input: MouseInput) {
        for index in 0..self.plugins.entries.len() {
            if self.plugins.entries[index].running.is_some() {
                self.call_plugin(index, |plugin, world| plugin.mouse(world, input));
            }
        }
    }

    /// Whether a plugin has the Scene View's left button (`plugin:Activate(true)`).
    pub(in crate::app) fn plugin_has_mouse(&self) -> bool {
        self.plugins
            .entries
            .iter()
            .filter_map(|entry| entry.running.as_ref())
            .any(Plugin::mouse_active)
    }

    /// The Plugins panel.
    pub(in crate::app) fn plugins_pane(&mut self, ui: &mut egui::Ui) {
        let project = self.project.as_ref().map(|project| project.folder.clone());
        match crate::plugins::show(ui, &self.plugins, project.as_deref()) {
            Some(PluginAction::Enable(index, enabled)) => {
                let key = self.plugins.entries[index].found.key.clone();
                self.plugins.set_enabled(&key, enabled);
                if enabled {
                    self.start_plugin(index);
                } else {
                    self.stop_plugin(index);
                    self.plugins.entries[index].errors.clear();
                }
            }
            Some(PluginAction::Reload(index)) => self.reload_plugin(index),
            Some(PluginAction::Rescan) => self.load_plugins(),
            None => {}
        }
    }

    /// The title of the plugin dock tab `pane`.
    pub(in crate::app) fn plugin_pane_title(&self, pane: u64) -> String {
        self.plugins
            .find_pane(pane)
            .map(|(index, widget)| {
                self.plugins.entries[index].widgets[widget]
                    .widget
                    .title
                    .clone()
            })
            .unwrap_or_else(|| "Plugin".to_owned())
    }

    /// The plugin dock tab `pane` was closed: its widget's `Enabled` goes off.
    pub(in crate::app) fn close_plugin_pane(&mut self, pane: u64) {
        if let Some((index, widget)) = self.plugins.find_pane(pane) {
            let gui = self.plugins.entries[index].widgets[widget].widget.gui;
            let _ = self
                .world
                .set_property(gui, "Enabled", axcenity_core::Value::Bool(false));
        }
    }

    /// Draws a plugin's dock tab: its UI, laid out and styled by the `ui`
    /// crate, and the pointer's presses, drags and clicks on it.
    pub(in crate::app) fn plugin_pane(
        &mut self,
        ui: &mut egui::Ui,
        pane: u64,
        render_state: Option<&RenderState>,
    ) {
        let Some((index, widget)) = self.plugins.find_pane(pane) else {
            ui.label("This plugin isn't running.");
            return;
        };
        let size = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let pixels_per_point = ui.ctx().pixels_per_point();
        let (width, height) = crate::animation::preview::Canvas::pixels(ui, rect);
        let (pointer, pressed, down, released, seconds, wheel) = ui.input(|input| {
            (
                input.pointer.latest_pos(),
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
                input.pointer.primary_released(),
                input.stable_dt,
                input.smooth_scroll_delta,
            )
        });
        let at = pointer
            .filter(|_| response.contains_pointer())
            .map(|pointer| (pointer - rect.min) * pixels_per_point)
            .map(|position| [position.x, position.y]);
        let playing = self.playing();
        let was_recording = self.world.is_recording_changes();
        self.world.record_changes();
        drop(self.take_world_changes());
        let entry = &mut self.plugins.entries[index];
        let view = &mut entry.widgets[widget];
        let input = UiInput {
            pointer: at,
            pressed: view.pressed,
            ..UiInput::default()
        };
        let layout = view.ui.layout(
            &self.world,
            (width, height),
            &entry.stylesheets,
            input,
            None,
            seconds,
        );
        let mut activated = None;
        if !playing && let Some(at) = at {
            if wheel != egui::Vec2::ZERO {
                let delta = -wheel * pixels_per_point;
                let _ = view
                    .ui
                    .wheel(&self.world, at, [delta.x, delta.y])
                    .apply(&mut self.world);
            }
            if pressed {
                let press = view.ui.press(&self.world, at);
                if !press.taken {
                    view.pressed = layout.hit_button(at[0], at[1]);
                }
                press.apply(&mut self.world);
            } else if down {
                let _ = view.ui.drag(&self.world, at).apply(&mut self.world);
            }
        }
        if released {
            let _ = view.ui.release(&self.world, at).apply(&mut self.world);
            if let Some(button) = view.pressed.take()
                && at.and_then(|[x, y]| layout.hit_button(x, y)) == Some(button)
                && view.ui.activate(&self.world, button)
            {
                activated = Some(button);
            }
        }
        if view.ui.scrolling() || response.hovered() {
            ui.ctx().request_repaint();
        }
        let events = view.ui.take_events();
        // The tab's backdrop is the panel, so unstyled UI reads as the editor's.
        let palette = theme::palette(ui.ctx());
        let [r, g, b, _] = palette.panel.to_normalized_gamma_f32();
        let backdrop = axcenity_render::UiDrawItem::Rectangle(axcenity_render::UiRectangle {
            position: [0.0, 0.0],
            size: [width as f32, height as f32],
            color: [r, g, b, 1.0],
            color_end: [r, g, b, 1.0],
            ..Default::default()
        });
        let items: Vec<_> = std::iter::once(backdrop)
            .chain(layout.draw_items.iter().cloned())
            .collect();
        match render_state {
            Some(render_state) => {
                let camera = axcenity_render::Camera {
                    position: axcenity_render::glam::Vec3::new(0.0, 5.0, 10.0),
                    look_at: axcenity_render::glam::Vec3::ZERO,
                    fov_y_degrees: 70.0,
                };
                let empty = World::new();
                let mut list = axcenity_scene::build_draw_list(&empty, &self.assets, camera);
                list.ui = items;
                axcenity_scene::attach_ui_textures(&mut list.ui, &self.assets);
                view.canvas.paint(ui, rect, render_state, &list);
            }
            None => crate::ui_designer::sketch(
                &ui.painter_at(rect),
                rect.min,
                pixels_per_point,
                items.iter(),
            ),
        }
        let changes = self.take_world_changes();
        if !was_recording {
            self.world.stop_recording_changes();
        }
        if changes.is_empty() && activated.is_none() && events.is_empty() {
            return;
        }
        self.call_plugin(index, |plugin, world| {
            plugin.fire_changes(world, &changes);
            for event in &events {
                match event {
                    axcenity_ui::UiEvent::MouseEnter(id) => {
                        plugin.fire_ui_event(world, *id, "MouseEnter")
                    }
                    axcenity_ui::UiEvent::MouseLeave(id) => {
                        plugin.fire_ui_event(world, *id, "MouseLeave")
                    }
                    axcenity_ui::UiEvent::PlaySound(_) => {}
                }
            }
            if let Some(button) = activated {
                plugin.activate_button(world, button);
            }
        });
    }

    /// The ribbon's Plugins group: each running plugin's toolbars.
    pub(in crate::app) fn plugin_ribbon_groups(&mut self, ui: &mut egui::Ui) {
        let playing = self.playing();
        let mut clicked = None;
        let mut any = false;
        for (index, entry) in self.plugins.entries.iter().enumerate() {
            let Some(plugin) = &entry.running else {
                continue;
            };
            for toolbar in plugin.toolbars() {
                if toolbar.buttons.is_empty() {
                    continue;
                }
                if any {
                    ui.add(egui::Separator::default().vertical().spacing(10.0));
                }
                any = true;
                ribbon::group(ui, &toolbar.name, |ui| {
                    for button in &toolbar.buttons {
                        let ribbon_button =
                            RibbonButton::new(button_icon(&button.icon), &button.name)
                                .selected(button.active);
                        let hover = if button.tooltip.is_empty() {
                            button.name.clone()
                        } else {
                            button.tooltip.clone()
                        };
                        if ui
                            .add_enabled(button.enabled && !playing, ribbon_button)
                            .on_hover_text(hover)
                            .on_disabled_hover_text("Plugins only run while editing.")
                            .clicked()
                        {
                            clicked = Some((index, button.id));
                        }
                    }
                });
            }
        }
        if !any {
            ribbon::group(ui, "Plugins", |ui| {
                if ui
                    .add(RibbonButton::new(icons::PUZZLE_PIECE, "Plugins"))
                    .on_hover_text("No plugin has made a toolbar. Open the Plugins panel.")
                    .clicked()
                {
                    self.layout.open(Pane::Plugins);
                }
            });
        }
        if let Some((index, button)) = clicked {
            self.click_plugin_button(index, button);
        }
    }
}

/// Whether a plugin widget's ScreenGui is `Enabled` (its tab shows).
fn widget_enabled(world: &World, gui: InstanceId) -> bool {
    world
        .get(gui)
        .and_then(|instance| instance.property("Enabled"))
        .and_then(axcenity_core::Value::as_bool)
        .unwrap_or(false)
}

/// The files in a plugins folder that reload a plugin, by their paths in it
/// (`align/init.lua`), for the plugins' watchers.
fn list_plugin_files(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        files.extend(
            axcenity_plugins::plugin_files(&entry.path())
                .into_iter()
                .map(|file| format!("{name}/{file}")),
        );
    }
    files
}

/// A plugin folder's stylesheets, parsed. Their warnings are left out: a
/// plugin's CSS only styles its own tabs.
fn read_stylesheets(folder: &Path) -> Vec<axcenity_ui::Stylesheet> {
    axcenity_plugins::plugin_files(folder)
        .into_iter()
        .filter(|path| axcenity_project::is_stylesheet(path))
        .filter_map(|path| {
            let source = std::fs::read_to_string(folder.join(&path)).ok()?;
            Some(axcenity_ui::parse_stylesheet(&path, &source).0)
        })
        .collect()
}
