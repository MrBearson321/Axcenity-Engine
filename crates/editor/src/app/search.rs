//! Universal search's index (Ctrl+K; `command_palette.rs` draws it).
//!
//! The index is a list of [`SearchProvider`]s, each filling one bucket of
//! rows. A provider says, with [`SearchProvider::stamp`], a number that
//! stays the same while what it lists does: the index asks it for rows again
//! only when that number changes, so creating, renaming, deleting or moving
//! an object, a script or an asset shows at once, and nothing is listed
//! again on a keystroke. Rows point at what they open with a
//! [`SearchTarget`]: commands through [`Command`] (the one command
//! registry, shared with the menus, the ribbon and the shortcut table),
//! objects by their [`InstanceId`], files by path.
//!
//! A new editor system makes itself searchable by adding a provider to
//! [`SearchIndex::new`] (or with [`SearchIndex::register`]); nothing else
//! here changes. Menu items and ribbon buttons are found already, as
//! [`Command`]s.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::command_palette::{Category, Entry, Hit};
use crate::find_in_scripts::FindAction;
use crate::ribbon_tabs::BuiltInTab;

use super::command_palette::{Command, OpenTarget};
use super::settings::{self, SettingsPage};
use super::*;
use axcenity_project as project;

/// What a row opens or runs.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum SearchTarget {
    Command(Command),
    Open(OpenTarget),
    /// A row of the Settings window: its page, and the row's name, which
    /// is scrolled to and highlighted.
    Setting(SettingsPage, &'static str),
    /// A property of a selected object, shown in Properties with its
    /// filter set to the name.
    Property(InstanceId, String),
}

/// A source of rows for universal search.
pub(super) trait SearchProvider {
    fn category(&self) -> Category;

    /// A number that's the same while what [`collect`](Self::collect)
    /// would list is. `None`: list again each time the palette opens (for
    /// rows that are cheap and change with the editor's state, such as
    /// which commands are enabled).
    fn stamp(&self, app: &EditorApp) -> Option<u64>;

    /// Adds this provider's rows to `out`.
    fn collect(&self, app: &EditorApp, ctx: &egui::Context, out: &mut Bucket);
}

/// One provider's rows and what they open, side by side.
#[derive(Default)]
pub(super) struct Bucket {
    pub entries: Vec<Entry>,
    pub targets: Vec<SearchTarget>,
    /// The provider's stamp when these were listed; `None` before.
    listed: Option<Option<u64>>,
}

impl Bucket {
    pub fn push(&mut self, entry: Entry, target: SearchTarget) {
        self.entries.push(entry);
        self.targets.push(target);
    }
}

/// The providers and their rows.
pub(super) struct SearchIndex {
    providers: Vec<Box<dyn SearchProvider>>,
    buckets: Vec<Bucket>,
    /// Goes up whenever a bucket is listed again, so the palette ranks
    /// again only then.
    generation: u64,
    /// How many times each provider listed its rows, for tests.
    #[cfg(test)]
    pub listings: Vec<usize>,
}

impl SearchIndex {
    /// The editor's providers, in the order their rows come when nothing
    /// is typed: commands, tools, settings, scripts, levels, assets, the
    /// selection's properties, then objects.
    pub fn new() -> Self {
        let mut index = Self::empty();
        index.register(Box::new(Commands(Category::Command)));
        index.register(Box::new(Commands(Category::Tool)));
        index.register(Box::new(Settings));
        index.register(Box::new(Scripts));
        index.register(Box::new(Levels));
        index.register(Box::new(Assets));
        index.register(Box::new(Properties));
        index.register(Box::new(Objects));
        index
    }

    /// No providers: what `EditorApp` holds while the real index is out
    /// being refreshed.
    fn empty() -> Self {
        Self {
            providers: Vec::new(),
            buckets: Vec::new(),
            generation: 0,
            #[cfg(test)]
            listings: Vec::new(),
        }
    }

    /// Adds a provider, whose rows show from the next refresh.
    pub fn register(&mut self, provider: Box<dyn SearchProvider>) {
        self.providers.push(provider);
        self.buckets.push(Bucket::default());
        #[cfg(test)]
        self.listings.push(0);
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Every bucket's rows, for ranking.
    pub fn entries(&self) -> Vec<&[Entry]> {
        self.buckets
            .iter()
            .map(|bucket| bucket.entries.as_slice())
            .collect()
    }

    pub fn target(&self, (bucket, index): Hit) -> Option<&SearchTarget> {
        self.buckets.get(bucket)?.targets.get(index)
    }

    /// The rows of `category`'s providers, for tests and Quick Open callers.
    #[cfg(test)]
    pub fn rows(&self, category: Category) -> Vec<(&Entry, &SearchTarget)> {
        self.providers
            .iter()
            .zip(&self.buckets)
            .filter(|(provider, _)| provider.category() == category)
            .flat_map(|(_, bucket)| bucket.entries.iter().zip(&bucket.targets))
            .collect()
    }
}

impl Default for SearchIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// A hash of anything, as a stamp.
fn stamp_of(value: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

impl EditorApp {
    /// Lists again the buckets whose stamp changed, and, when `opening`,
    /// those without one. Cheap when nothing changed: the stamps are a few
    /// counters, and the objects' one adds up their revisions.
    pub(super) fn refresh_search(&mut self, ctx: &egui::Context, opening: bool) {
        self.asset_browser.poll();
        let mut index = std::mem::replace(&mut self.search, SearchIndex::empty());
        for (at, provider) in index.providers.iter().enumerate() {
            let stamp = provider.stamp(self);
            let bucket = &mut index.buckets[at];
            let stale = match (bucket.listed, stamp) {
                (None, _) => true,
                (Some(_), None) => opening,
                (Some(listed), Some(_)) => listed != stamp,
            };
            if !stale {
                continue;
            }
            bucket.entries.clear();
            bucket.targets.clear();
            provider.collect(self, ctx, bucket);
            debug_assert!(
                bucket
                    .entries
                    .iter()
                    .all(|entry| entry.category == provider.category()),
                "a provider's rows are all of its category"
            );
            bucket.listed = Some(stamp);
            index.generation += 1;
            #[cfg(test)]
            {
                index.listings[at] += 1;
            }
        }
        self.search = index;
    }

    /// Opens or runs what a row points at.
    pub(super) fn run_search_target(&mut self, ctx: &egui::Context, target: SearchTarget) {
        match target {
            SearchTarget::Command(command) => self.run_command(ctx, command),
            SearchTarget::Open(target) => self.open_target(ctx, target),
            SearchTarget::Setting(page, name) => {
                self.settings.open = true;
                self.settings.page = page;
                settings::find_setting(ctx, name);
            }
            SearchTarget::Property(id, name) => {
                let world = shown(&self.world, &self.play);
                if !world.contains(id) {
                    return;
                }
                if !self.selection.contains(&id) {
                    self.select_from_viewport(Some(id));
                }
                self.layout.open(Pane::Properties);
                crate::properties::set_filter(ctx, &name);
            }
        }
    }

    /// The palette's keys and window, Quick Open and Find in Scripts, run
    /// before the other shortcuts so their keys reach them first.
    pub(super) fn palette_frame(&mut self, ctx: &egui::Context) {
        use crate::shortcuts::{Action, consume};
        // Shift first: egui counts Ctrl+Shift+P as Ctrl+P too.
        let (commands, find, quick, search) = ctx.input_mut(|input| {
            let commands = consume(input, Action::Palette);
            let find = consume(input, Action::FindInScripts);
            let quick = consume(input, Action::QuickOpen);
            (commands, find, quick, consume(input, Action::Search))
        });
        if commands {
            self.palette.toggle_commands();
        }
        if quick || search {
            self.palette.toggle();
        }
        if find && self.project.is_some() {
            self.find.show_and_focus();
            self.search_scripts();
        }
        if self.palette.open {
            let opening = !self.palette_was_open;
            self.refresh_search(ctx, opening);
            let entries = self.search.entries();
            let picked = self.palette.ui(ctx, &entries, self.search.generation());
            if let Some(hit) = picked
                && let Some(target) = self.search.target(hit).cloned()
            {
                self.run_search_target(ctx, target);
            }
        }
        self.palette_was_open = self.palette.open;
        let can_replace = !self.playing();
        match self.find.ui(ctx, can_replace) {
            Some(FindAction::Search) => self.search_scripts(),
            Some(FindAction::GoTo(hit)) => self.go_to_hit(&hit),
            Some(FindAction::ReplaceAll) => self.replace_all_in_scripts(),
            None => {}
        }
    }
}

/// Menu and ribbon commands (`Category::Command`), or the editor tools
/// among them (`Category::Tool`): [`EditorApp::palette_commands`], with each
/// command's description and synonyms.
struct Commands(Category);

impl SearchProvider for Commands {
    fn category(&self) -> Category {
        self.0
    }

    fn stamp(&self, _: &EditorApp) -> Option<u64> {
        None
    }

    fn collect(&self, app: &EditorApp, ctx: &egui::Context, out: &mut Bucket) {
        for command in app.palette_commands(ctx) {
            let category = command_category(&command.command);
            if category != self.0 {
                continue;
            }
            let mut entry = command.entry.in_category(category);
            if entry.description.is_empty() {
                entry.description = command_description(&command.command);
            }
            if entry.icon.is_empty() {
                entry.icon = command_icon(&command.command, category);
            }
            for synonym in synonyms(&entry.title, &entry.detail) {
                if !entry.aliases.iter().any(|alias| alias == synonym) {
                    entry.aliases.push((*synonym).to_owned());
                }
            }
            out.push(entry, SearchTarget::Command(command.command));
        }
    }
}

/// The icon a command's row shows: its tool's or panel's, else one for
/// its kind.
fn command_icon(command: &Command, category: Category) -> &'static str {
    match command {
        Command::Tool(tool) => theme::tool_icon(*tool),
        Command::TogglePane(pane) => pane.icon(),
        _ if category == Category::Tool => icons::WRENCH,
        _ => icons::COMMAND,
    }
}

/// Whether `command` is one of the editor's tools rather than a command.
pub(super) fn command_category(command: &Command) -> Category {
    match command {
        Command::Tool(_)
        | Command::TerrainTool(_)
        | Command::TerrainStudio
        | Command::ModelStudio
        | Command::Constraint(_)
        | Command::RibbonTab(_) => Category::Tool,
        Command::TogglePane(pane) if TOOL_PANES.contains(pane) => Category::Tool,
        _ => Category::Command,
    }
}

/// The panels that are tools of their own (editors, studios, monitors),
/// rather than the Explorer, Properties, Console and Assets.
const TOOL_PANES: [Pane; 16] = [
    Pane::Animation,
    Pane::UiDesigner,
    Pane::Terrain,
    Pane::Profiler,
    Pane::Mixer,
    Pane::AssetCheck,
    Pane::BehaviorTree,
    Pane::Dialogue,
    Pane::DataTable,
    Pane::Material,
    Pane::Tags,
    Pane::Levels,
    Pane::Sequencer,
    Pane::Tests,
    Pane::ModelStudio,
    Pane::Toolbox,
];

/// Other words people search for a command by, matched like its name.
/// Keyed by words of the title (or where it lives), so families of
/// commands share them.
fn synonyms(title: &str, detail: &str) -> &'static [&'static str] {
    let has = |word: &str| title.contains(word);
    if has("Delete") {
        &["Remove", "Erase", "Destroy"]
    } else if has("Duplicate") {
        &["Copy", "Clone"]
    } else if has("Rename") {
        &["Name"]
    } else if has("Build Game") {
        &["Export", "Package", "Publish", "Release"]
    } else if has("Import") {
        &[
            "Load model",
            "Add file",
            "Mesh",
            "Texture",
            "FBX",
            "glTF",
            "OBJ",
        ]
    } else if title == "Play" || title == "Continue" {
        &["Run", "Test", "Start game"]
    } else if title == "Stop" {
        &["End", "Quit game"]
    } else if has("Settings") {
        &["Preferences", "Options", "Config"]
    } else if has("Undo") {
        &["Revert", "Back"]
    } else if has("Save") {
        &["Write", "Store"]
    } else if has("Check Assets") {
        &["Missing files", "Broken references", "Validate"]
    } else if has("Collision Groups") {
        &["Physics", "Layers", "Collide"]
    } else if has("Weld") || has("Constraint") {
        &["Physics", "Joint", "Attach"]
    } else if has("Overlay: Colliders") {
        &["Physics", "Collision"]
    } else if has("Terrain") {
        &["Landscape", "Ground", "Heightmap"]
    } else if has("Material") {
        &["Surface", "Texture", "Shader"]
    } else if has("Animation") {
        &["Animate", "Keyframes", "Clips"]
    } else if has("UI Designer") || detail.contains("Ribbon › UI") {
        &["Interface", "GUI", "HUD"]
    } else if detail.contains("Lights & Sky") || has("Light") {
        &["Lighting", "Lamp", "Illumination"]
    } else if has("Theme") || has("Appearance") {
        &["Dark mode", "Light mode", "Colors"]
    } else if has("Profiler") {
        &["Performance", "FPS", "Frame time"]
    } else if has("Version History") || has("Checkpoint") {
        &["Backup", "Restore", "Snapshot"]
    } else {
        &[]
    }
}

/// What `command` does, for the line under the palette's list: the
/// ribbon's own hover words where there are some, else a sentence here.
fn command_description(command: &Command) -> String {
    let text: &str = match command {
        Command::Insert(what) => return crate::explorer::insert_hint(*what),
        Command::InsertScript(class) => crate::explorer::script_hover(*class),
        Command::Solid(action) => action.hover(),
        Command::TerrainTool(tool) => tool.hint(),
        Command::ViewMode(mode) => mode.hint(),
        Command::Tool(tool) => return tool_description(*tool),
        Command::NewProject => "Start a project from a template.",
        Command::OpenProject => "Open a project folder.",
        Command::Save => "Save the world, scripts and project files.",
        Command::VersionHistory => {
            "See the project's saved checkpoints and restore one, or compare it with now."
        }
        Command::SaveCheckpoint => "Keep a named copy of the project as it is now.",
        Command::BackUpProject => "Write a zip of the whole project.",
        Command::Input => {
            "The project's input map: actions and the keys, buttons and sticks that do them."
        }
        Command::CloseProject => "Close this project and go back to the start screen.",
        Command::Import => "Bring models, images, sounds or prefabs into the project's assets.",
        Command::BuildGame(_) => {
            "Package the game so it runs without the editor, for the chosen computer."
        }
        Command::LoadingScreen(_) | Command::WaitForScript => {
            "What a built game shows while it loads."
        }
        Command::DeveloperConsole => "Let players open the Developer Console (F9) in built games.",
        Command::OpenStylesheet => "Open a UI stylesheet to edit.",
        Command::NewStylesheet => "Make a new UI stylesheet.",
        Command::CheckAssets => {
            "Look for missing files, broken references, failed imports and broken dependencies."
        }
        Command::SetUpAutocomplete => {
            "Write the files VS Code needs to complete Axcenity's API in your scripts."
        }
        Command::Settings => {
            "The editor's appearance, layout, graphics, shortcuts and accessibility."
        }
        Command::Quit => "Close the editor.",
        Command::Undo => "Take back the last change.",
        Command::Redo => "Make the change Undo took back again.",
        Command::Rename => "Give what's selected a new name.",
        Command::Duplicate => "Make a copy of what's selected, next to it.",
        Command::Delete => "Remove what's selected from the world.",
        Command::Theme(_) | Command::Appearance => "Change the editor's colours and size.",
        Command::TogglePane(pane) => return format!("Show or hide the {} panel.", pane.label()),
        Command::ShowScene => "Bring the Scene View to the front.",
        Command::ResetLayout => "Put every panel back where it starts.",
        Command::LayoutPreset(preset) => return preset.about().to_owned(),
        Command::SavedLayout(name) => return format!("Switch to the layout you saved as {name}."),
        Command::SaveLayout => "Keep where the panels are now, by name, to switch back to later.",
        Command::LightCones => "Show every light's cone, not only the selected one's.",
        Command::ViewCube => "The cube in the viewport's corner that turns the camera to a face.",
        Command::Grid => "Lines and distances in studs on the baseplate, while editing.",
        Command::Overlay(_) => "Draw a debugging overlay over the viewport.",
        Command::NewRibbonTab => "Make your own ribbon tab from any of the ribbon's groups.",
        Command::RibbonTab(tab) => return format!("Show the ribbon's {} tab.", tab_label(*tab)),
        Command::Learn => "Tutorials and notes on each part of the editor.",
        Command::About => "The editor's version and licences.",
        Command::PanicDuringPlay => "Debug builds only: make Play fail, to test error handling.",
        Command::Play => {
            "Run the game in the editor. Changes made while playing are undone by Stop."
        }
        Command::Stop => "Stop the game and go back to editing.",
        Command::Snap(_) => "Turn one of the Snapping group's switches on or off.",
        Command::CollisionGroups => "Which groups of parts collide with which.",
        Command::BakeProbes => "Work out the light probes' lighting again.",
        Command::Weld => "Join the selected parts so they move as one.",
        Command::Constraint(Some(_)) => "Pick two surfaces to join with this constraint.",
        Command::Constraint(None) => "Stop picking surfaces for a constraint.",
        Command::InsertControls => "Add an on-screen controls menu for touch and gamepad.",
        Command::TerrainStudio => "Sculpt, paint, scatter foliage on and generate terrain.",
        Command::ModelStudio => "Make and edit models: meshes, UVs, painting, sculpting and rigs.",
        Command::PluginButton { .. } => "A button a plugin added.",
        Command::Palette => "Run any menu or ribbon command by typing part of its name.",
        Command::QuickOpen | Command::Search => {
            "Find anything: commands, tools, objects, assets, scripts, levels and settings."
        }
        Command::FindInScripts => "Search every script for some text, and replace it.",
    };
    text.to_owned()
}

fn tab_label(tab: BuiltInTab) -> &'static str {
    tab.label()
}

/// What a transform or build tool does.
fn tool_description(tool: Tool) -> String {
    let text = match tool {
        Tool::Select => "Click to select. Drag to select everything in a box.",
        Tool::Move => "Drag the arrows to move what's selected along an axis.",
        Tool::Scale => "Drag the handles to resize what's selected.",
        Tool::Rotate => "Drag the rings to turn what's selected.",
        Tool::Transform => "Move, resize and rotate handles together.",
        Tool::Paint => "Click parts to paint them the picked colour.",
        Tool::Material => "Click parts to give them the picked material.",
        Tool::Anchor => "Click parts to anchor them in place, or free them.",
        Tool::Lighting => "Click parts to add or change the light in them.",
        Tool::Emission => "Click parts to make them glow.",
        Tool::Angular => "Place copies of a part at an angle around a point.",
    };
    match tool.key() {
        Some(key) => format!("{text} Key: {}.", key.name()),
        None => text.to_owned(),
    }
}

/// The Settings window's rows ([`settings::SEARCHABLE`]).
struct Settings;

impl SearchProvider for Settings {
    fn category(&self) -> Category {
        Category::Setting
    }

    fn stamp(&self, app: &EditorApp) -> Option<u64> {
        Some(u64::from(app.rebuilds.source.is_some()))
    }

    fn collect(&self, app: &EditorApp, _: &egui::Context, out: &mut Bucket) {
        for row in settings::SEARCHABLE {
            if row.page == SettingsPage::Engine && app.rebuilds.source.is_none() {
                continue;
            }
            let mut entry = Entry::new(
                format!("setting {}", row.name),
                row.name,
                format!("Settings › {}", row.page.label()),
            )
            .in_category(Category::Setting);
            entry.aliases = row
                .aliases
                .iter()
                .map(|alias| (*alias).to_owned())
                .collect();
            entry.description = row.description.to_owned();
            entry.icon = row.page.icon();
            out.push(entry, SearchTarget::Setting(row.page, row.name));
        }
    }
}

/// The project's scripts.
struct Scripts;

impl SearchProvider for Scripts {
    fn category(&self) -> Category {
        Category::Script
    }

    fn stamp(&self, app: &EditorApp) -> Option<u64> {
        let paths: Vec<&str> = app
            .project
            .iter()
            .flat_map(|project| {
                project
                    .scripts
                    .iter()
                    .map(|script| script.path.as_str())
                    .chain(project.object_files.keys().map(String::as_str))
            })
            .collect();
        Some(stamp_of((&app.project.as_ref().map(|p| &p.folder), paths)))
    }

    fn collect(&self, app: &EditorApp, _: &egui::Context, out: &mut Bucket) {
        let Some(project) = &app.project else {
            return;
        };
        for script in &project.scripts {
            let class = project::script_class(&script.path);
            let mut entry = Entry::new(
                format!("script {}", script.path),
                project::script_name(&script.path),
                &script.path,
            )
            .in_category(Category::Script);
            entry.icon = theme::class_icon(class);
            entry.description = format!("A {class}. Opens in its own tab.");
            out.push(
                entry,
                SearchTarget::Open(OpenTarget::Script(script.path.clone())),
            );
        }
        // The code of scripts inside objects (G2), once per file.
        for path in project.object_files.keys() {
            if project::is_block_script(path) {
                continue;
            }
            let class = project::script_class(path);
            let mut entry = Entry::new(format!("script {path}"), project::script_name(path), path)
                .in_category(Category::Script);
            entry.icon = theme::class_icon(class);
            entry.description =
                format!("The code of a {class} inside an object. Opens in its own tab.");
            out.push(entry, SearchTarget::Open(OpenTarget::Script(path.clone())));
        }
    }
}

/// The project's levels. Read from disk when the palette opens.
struct Levels;

impl SearchProvider for Levels {
    fn category(&self) -> Category {
        Category::Level
    }

    fn stamp(&self, _: &EditorApp) -> Option<u64> {
        None
    }

    fn collect(&self, app: &EditorApp, _: &egui::Context, out: &mut Bucket) {
        let Some(project) = &app.project else {
            return;
        };
        let start = project.start_level.clone();
        let mut entry = Entry::new(format!("level {start}"), &start, "Level (start)")
            .in_category(Category::Level);
        entry.icon = icons::MAP_TRIFOLD;
        entry.description = "The level the game starts in. Opening it asks to save first.".into();
        out.push(entry, SearchTarget::Open(OpenTarget::Level(None)));
        for name in project::level_names(&project.folder) {
            if name == start {
                continue;
            }
            let mut entry =
                Entry::new(format!("level {name}"), &name, "Level").in_category(Category::Level);
            entry.icon = icons::MAP_TRIFOLD;
            entry.description = "Opening a level asks to save first.".into();
            out.push(entry, SearchTarget::Open(OpenTarget::Level(Some(name))));
        }
    }
}

/// The Assets panel's files: the project's and the global assets it
/// hasn't got, as its scan last found them (no second walk of the disk).
struct Assets;

impl SearchProvider for Assets {
    fn category(&self) -> Category {
        Category::Asset
    }

    fn stamp(&self, app: &EditorApp) -> Option<u64> {
        Some(app.asset_browser.generation())
    }

    fn collect(&self, app: &EditorApp, _: &egui::Context, out: &mut Bucket) {
        for asset in app.asset_browser.listed() {
            if project::is_model_part(&asset.path) {
                continue;
            }
            let kind = asset.kind.label();
            let file = asset.path.rsplit('/').next().unwrap_or(&asset.path);
            let mut entry = Entry::new(format!("asset {}", asset.path), file, &asset.path)
                .in_category(Category::Asset);
            entry.icon = asset.kind.icon();
            // Its kind is another name for it, so "texture" or "sound"
            // lists every one.
            entry.aliases.push(format!("{kind} {}", asset.name));
            entry.description = if app.asset_browser.is_global(&asset.path)
                && !app.asset_browser.is_local(&asset.path)
            {
                format!("{kind}, from the global assets. Shows it in the Assets panel.")
            } else {
                format!("{kind}. Shows it in the Assets panel, or opens it in its tab.")
            };
            out.push(
                entry,
                SearchTarget::Open(OpenTarget::Asset(asset.path.clone())),
            );
        }
    }
}

/// The properties of what's selected (the last selected, as Properties
/// shows), so "transparency" goes straight to the field.
struct Properties;

impl SearchProvider for Properties {
    fn category(&self) -> Category {
        Category::Property
    }

    fn stamp(&self, app: &EditorApp) -> Option<u64> {
        let world = shown(&app.world, &app.play);
        let selected = app.selection.last().copied();
        let revision = selected
            .and_then(|id| world.get(id))
            .map(|instance| instance.revision_without_motion());
        Some(stamp_of((selected, revision, app.play.is_some())))
    }

    fn collect(&self, app: &EditorApp, _: &egui::Context, out: &mut Bucket) {
        let world = shown(&app.world, &app.play);
        let Some(&id) = app.selection.last() else {
            return;
        };
        let Some(instance) = world.get(id) else {
            return;
        };
        let class = instance.class();
        for section in crate::properties::sections(instance) {
            for (name, _) in &section.properties {
                let mut entry = Entry::new(
                    format!("property {name}"),
                    *name,
                    format!("{} › {}", instance.name(), section.title),
                )
                .in_category(Category::Property);
                entry.icon = icons::SLIDERS_HORIZONTAL;
                entry.description = crate::properties::description(class, name)
                    .unwrap_or("Shows it in Properties.")
                    .to_owned();
                out.push(entry, SearchTarget::Property(id, (*name).to_owned()));
            }
        }
    }
}

/// Every Instance in the world shown (the session's during Play).
struct Objects;

impl SearchProvider for Objects {
    fn category(&self) -> Category {
        Category::Object
    }

    /// The tree's structure version (creating, deleting and moving), and
    /// the sum of every Instance's revision without motion (renaming):
    /// revisions only go up, so the sum changes whenever one does.
    fn stamp(&self, app: &EditorApp) -> Option<u64> {
        let world = shown(&app.world, &app.play);
        let revisions = world
            .descendants_iter(world.root())
            .filter_map(|id| world.get(id))
            .fold(0u64, |sum, instance| {
                sum.wrapping_add(instance.revision_without_motion())
            });
        Some(stamp_of((
            std::ptr::from_ref(world) as usize,
            app.play.is_some(),
            world.structure_version(),
            revisions,
        )))
    }

    fn collect(&self, app: &EditorApp, _: &egui::Context, out: &mut Bucket) {
        let world = shown(&app.world, &app.play);
        for id in world.descendants_iter(world.root()) {
            let Some(instance) = world.get(id) else {
                continue;
            };
            let path = world.full_name(id).unwrap_or_default();
            let class = instance.class();
            let mut entry = Entry::new(
                format!("instance {path}"),
                instance.name(),
                format!("{class} · {path}"),
            )
            .in_category(Category::Object);
            entry.icon = theme::class_icon(class);
            entry.description = "Selects it and shows it in the Explorer.".into();
            out.push(entry, SearchTarget::Open(OpenTarget::Instance(id)));
        }
    }
}
