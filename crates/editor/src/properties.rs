//! The Properties panel: the selected Instance's name, class and properties,
//! in groups like Roblox Studio's, each with a control that fits its type.
//!
//! The panel never changes the world itself. [`show`] returns the [`Edit`]
//! the user made, and the app applies it, which keeps the panel easy to test
//! and gives one place to notice unsaved changes.

use std::collections::HashMap;

use axcenity_core::{
    CFrame, ClassName, Color3, EnumItem, Instance, InstanceId, Material, PartType, ParticleType,
    PhysicalProperties, TextureChannel, ToneMapping, UDim2, Value, Vector2, Vector3, World,
    is_attribute_name,
};

use axcenity_assets::{Assets, is_asset_reference};
use eframe::egui;

use crate::theme::{self, icons};
pub use axcenity_editor_kit::widgets::transparency_slider;
pub use axcenity_editor_tools::edit::Edit;
use axcenity_editor_tools::edit::Field;

pub use crate::gizmo::MIN_SIZE;
/// Numbers show at most this many decimals, so a typed 0.3 doesn't show up
/// as 0.30000001 (the closest a 32-bit float gets).
const MAX_DECIMALS: usize = 3;
/// The width a UDim2's two axes need side by side.
const UDIM2_ONE_ROW: f32 = 270.0;

/// Whether the Properties panel lets you rename this Instance. Services keep
/// their names, and a Script is named after its file, so renaming it here
/// would be lost on the next load.
pub fn can_rename(instance: &Instance) -> bool {
    !instance.class().is_service()
}

/// Whether the Name box in this panel can rename `instance`. Renaming a
/// script renames its file, which shouldn't happen at every key typed, so
/// Scripts are renamed in the Explorer instead, all at once.
pub fn can_rename_here(instance: &Instance) -> bool {
    can_rename(instance) && !crate::tree::is_file_script(instance)
}

/// Draws the panel for `selected` and returns the edit made this frame, if any.
/// While `playing`, `world` is the game's, and the panel says that changes
/// last only until Stop unless kept: `kept` lists the fields edited during
/// Play and whether each is kept, and each gets a Keep pin by its name.
/// `more` draws further sections (the Style section) at the end, in the
/// same scrolling area.
pub fn show(
    ui: &mut egui::Ui,
    world: &World,
    assets: &Assets,
    selected: Option<InstanceId>,
    playing: bool,
    kept: &[(Field, bool)],
    more: impl FnOnce(&mut egui::Ui),
) -> Option<Edit> {
    let palette = theme::palette(ui.ctx());
    if playing {
        notice(
            ui,
            icons::INFO,
            "Changes made during Play are undone at Stop, unless you keep them: click the pin \
             beside a value you changed, or right-click the object in the Explorer and pick \
             Keep changes.",
        );
    }
    let pin = |field: Field| {
        kept.iter()
            .find(|(edited, _)| *edited == field)
            .map(|(_, kept)| *kept)
    };
    let mut edit = None;
    // The scroll area fills the panel even when nothing is selected. Without
    // it the panel would shrink to fit one line of text and stay that small.
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            let Some((id, instance)) = selected.and_then(|id| Some((id, world.get(id)?))) else {
                crate::widgets::empty_state(
                    ui,
                    icons::CURSOR_CLICK,
                    "Nothing selected",
                    "Select something in the Explorer or the viewport to see its properties.",
                    None,
                );
                return;
            };
            // What's selected, so it's clear what the fields below change.
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(theme::class_icon(instance.class()))
                        .size(16.0)
                        .color(theme::class_color(instance.class(), ui.visuals())),
                );
                // The class keeps its room; a long name ends in "…".
                let class = ui.painter().layout_no_wrap(
                    instance.class().as_str().to_owned(),
                    egui::FontId::proportional(theme::type_scale::SMALL),
                    palette.text_weak,
                );
                let name_room = ui.available_width() - class.size().x - ui.spacing().item_spacing.x;
                ui.scope(|ui| {
                    ui.set_max_width(name_room.max(24.0));
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(instance.name())
                                .family(theme::heading_family())
                                .color(palette.text_strong),
                        )
                        .truncate(),
                    );
                });
                ui.label(class);
            });
            if !playing && crate::animation::can_animate(instance.class()) {
                let icon = theme::AnimKind::Animator.rich_icon(ui.visuals());
                if ui
                    .button((icon, format!("  Animate {}", instance.name())))
                    .on_hover_text(
                        "Give it an Animator, a new clip and an Animation playing it, and open the clip with Record on",
                    )
                    .clicked()
                {
                    edit = Some(Edit::Animate);
                }
            }
            if !playing
                && instance.class() == ClassName::Ragdoll
                && ui
                    .button(theme::with_icon(icons::PERSON_SIMPLE, "Build"))
                    .on_hover_text(
                        "Make a capsule for each main bone of the MeshPart's skeleton and a joint between them, with a humanoid's limits. Replaces what's in the Ragdoll.",
                    )
                    .clicked()
            {
                edit = Some(Edit::BuildRagdoll);
            }
            if !playing
                && instance.class() == ClassName::Fracture
                && ui
                    .button(theme::with_icon(icons::HAMMER, "Generate"))
                    .on_hover_text(
                        "Cut the part into Pieces convex pieces by Seed and Pattern, and save them as <name>.fracture.glb. Generate again after changing the part or these settings.",
                    )
                    .clicked()
            {
                edit = Some(Edit::GenerateFracture);
            }
            let generator = axcenity_core::spline::is_generator(instance.class());
            if !playing
                && generator
                && ui
                    .button(theme::with_icon(icons::PACKAGE, "Bake"))
                    .on_hover_text(
                        "Keep what it made as ordinary Instances, in a Model beside the Spline (or where a building stands), and remove it. A SplineMesh's bent pieces are saved as <name>.spline.glb.",
                    )
                    .clicked()
            {
                edit = Some(Edit::BakeSpline);
            }
            if !playing
                && ((generator && instance.class() != ClassName::BuildingGenerator)
                    || instance.class() == ClassName::Spline)
                && ui
                    .button(theme::with_icon(icons::MOUNTAINS, "Flatten Terrain Along"))
                    .on_hover_text(
                        "Flatten a strip of the Terrain under the spline to the curve's height, as wide as its Spline Mesh's road and a stud more each side (8 studs without one). One undo step.",
                    )
                    .clicked()
            {
                edit = Some(Edit::FlattenTerrainAlong);
            }
            if !playing && instance.class() == ClassName::TerrainPath {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(theme::with_icon(icons::MOUNTAINS, "Apply to Terrain"))
                        .on_hover_text(
                            "Flatten the ground under its Spline to the curve (with Flatten) and paint its Layer, Width wide, blending into the ground over Falloff. One undo step.",
                        )
                        .clicked()
                    {
                        edit = Some(Edit::ApplyTerrainPath);
                    }
                    if ui
                        .button(theme::with_icon(icons::PATH, "Route"))
                        .on_hover_text(
                            "Lay the Spline's points over the terrain from its first point to its last, climbing no steeper than Max Grade: round a hill or up it in bends. One undo step.",
                        )
                        .clicked()
                    {
                        edit = Some(Edit::RouteTerrainPath);
                    }
                });
            }
            let has_mesh = instance
                .property("MeshId")
                .and_then(Value::as_str)
                .is_some_and(|id| !id.is_empty());
            if !playing
                && instance.class() == ClassName::MeshPart
                && has_mesh
                && ui
                    .button(theme::with_icon(icons::POLYGON, "Edit in Model Studio"))
                    .on_hover_text("Edit its model's meshes in the Model Studio tab. Saving writes a .glb.")
                    .clicked()
            {
                edit = Some(Edit::OpenInModelStudio);
            }
            if !playing
                && instance.class() == ClassName::MeshPart
                && has_mesh
                && ui
                    .button(theme::with_icon(icons::ARROW_SQUARE_OUT, "Edit in Blender"))
                    .on_hover_text(
                        "Open its model's .glb in Blender. Save there (Ctrl+S) to send it back.",
                    )
                    .clicked()
            {
                edit = Some(Edit::OpenInBlender);
            }
            ui.add_space(2.0);
            // Filter properties: shows only the rows whose names (or whose
            // section's title) match, with their sections open. Kept from
            // one selection to the next, like Roblox Studio's.
            let filter_id = egui::Id::new("properties filter");
            let mut filter: String = ui.data_mut(|d| d.get_temp(filter_id)).unwrap_or_default();
            let width = (ui.available_width() - crate::widgets::SEARCH_FIELD_MARGINS).max(60.0);
            crate::widgets::search_field(ui, &mut filter, "Filter properties", width)
                .on_hover_text("Show only the properties whose names have this in them");
            ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
            let filter = filter.trim();
            let filtering = !filter.is_empty();
            let open = filtering.then_some(true);
            let shows = |title: &str, name: &str| {
                !filtering || matches_filter(name, filter) || matches_filter(title, filter)
            };
            let mut shown_any = false;
            // Only `Locked` itself can be changed on something locked.
            let locked = !playing && world.is_locked(id);
            if locked {
                ui.weak(theme::with_icon(
                    icons::LOCK,
                    "Locked. Turn Locked off to change it.",
                ));
            }
            // Widgets remember things such as half-typed numbers by ID. Mixing in
            // the selection gives each Instance its own, so nothing typed for one
            // part turns up on the next one selected.
            ui.push_id(id, |ui| {
                let class = instance.class();
                // The header already says the class, so its own row only
                // shows when the filter asks for it.
                let data = [
                    shows("Data", "Name"),
                    filtering && shows("Data", "ClassName"),
                ];
                shown_any |= data.contains(&true);
                section(ui, "Data", open, data.contains(&true), |ui| {
                  if data[0] {
                    if let Some(keep) = field_name_with_pin(ui, class, "Name", pin(Field::Name)) {
                        edit = Some(Edit::Keep(Field::Name, keep));
                    }
                    let mut name = instance.name().to_string();
                    if can_rename_here(instance) && !locked {
                        if ui.text_edit_singleline(&mut name).changed() {
                            edit = Some(Edit::Name(name));
                        }
                    } else {
                        let hint = if locked {
                            "It's locked. Turn Locked off to rename it."
                        } else if instance.class().is_script() {
                            "Rename a script in the Explorer (F2). That renames its file too."
                        } else {
                            "Services can't be renamed."
                        };
                        ui.add_enabled(false, egui::TextEdit::singleline(&mut name))
                            .on_disabled_hover_text(hint);
                    }
                    ui.end_row();
                  }
                  if data[1] {
                    described_name(ui, class, "ClassName");
                    ui.label(instance.class().as_str());
                    ui.end_row();
                  }
                });

                // In a placed prefab copy, what it sets differently (Phase 49).
                let overridden = match playing {
                    true => Default::default(),
                    false => axcenity_core::prefab::place(world, id)
                        .map(|place| place.here().properties)
                        .unwrap_or_default(),
                };
                // What a new one of its class starts with, to offer ↺ on
                // what differs (not during Play, whose changes go at Stop).
                let defaults: HashMap<&str, Value> = if playing {
                    HashMap::new()
                } else {
                    instance.class().default_properties().into_iter().collect()
                };
                // One property's row: its name (with ↺, the prefab's ↺ or
                // Play's pin) and its control.
                let mut row = |ui: &mut egui::Ui, name: &'static str, value: Value| {
                    let field = Field::Property(name);
                    let reset = defaults
                        .get(name)
                        .filter(|default| resettable(default) && **default != value)
                        .filter(|_| !locked || name == "Locked");
                    if overridden.contains(name) {
                        if let Some(made) = override_name(ui, name) {
                            edit = Some(made);
                        }
                    } else if let Some(default) = reset {
                        if field_name_with_reset(ui, class, name, default) {
                            edit = Some(Edit::Property(name, default.clone()));
                        }
                    } else if let Some(keep) =
                        field_name_with_pin(ui, class, name, pin(field))
                    {
                        edit = Some(Edit::Keep(field, keep));
                    }
                    // A start and end pair gives way to its curve while the
                    // curve is set.
                    let replaced = instance.class() == ClassName::ParticleEmitter
                        && groups::replacing_curve(name)
                            .and_then(|curve| instance.property(curve))
                            .is_some_and(|curve| match curve {
                                Value::NumberSequence(sequence) => !sequence.is_empty(),
                                Value::ColorSequence(sequence) => !sequence.is_empty(),
                                _ => false,
                            });
                    let enabled = (!locked || name == "Locked") && !replaced;
                    let made = ui.add_enabled_ui(enabled, |ui| match value {
                        // Who sits there is up to the game, not the editor.
                        Value::Instance(target)
                            if name == "Occupant"
                                && instance.class().is_seat() =>
                        {
                            let who = target
                                .and_then(|target| world.full_name(target))
                                .unwrap_or_else(|| "None".to_owned());
                            ui.label(who).on_hover_text(
                                "The controller sitting here during Play: set by \
                                 walking into the seat or by seat:Sit(controller).",
                            );
                            None
                        }
                        // An ObjectValue's Value picks from the world.
                        Value::Instance(target) => object_control(ui, world, id, target)
                            .map(|target| Edit::Property(name, Value::Instance(target))),
                        // Shows the material's values while it has none.
                        Value::PhysicalProperties(custom) => {
                            physical_control(ui, instance, custom).map(|custom| {
                                Edit::Property(name, Value::PhysicalProperties(custom))
                            })
                        }
                        value => value_control(ui, name, value, assets),
                    })
                    .inner;
                    if made.is_some() {
                        edit = made;
                    }
                    ui.end_row();
                };
                // Rare settings go in Advanced, at the end and closed until
                // opened, so the common ones come first.
                let mut advanced = Vec::new();
                for mut group in sections(instance) {
                    group.properties.retain(|(name, _)| shows(group.title, name));
                    let (rare, common): (Vec<_>, Vec<_>) = group
                        .properties
                        .into_iter()
                        .partition(|(name, _)| is_advanced(class, name));
                    advanced.extend(rare);
                    if common.is_empty() {
                        continue;
                    }
                    shown_any = true;
                    section(ui, group.title, open, true, |ui| {
                        for (name, value) in common {
                            row(ui, name, value);
                        }
                    });
                }
                if !advanced.is_empty() {
                    shown_any = true;
                    let changed = advanced
                        .iter()
                        .filter(|(name, value)| {
                            defaults.get(name).is_some_and(|default| default != value)
                        })
                        .count();
                    let title = match changed {
                        0 => ADVANCED.to_owned(),
                        n => format!("{ADVANCED} · {n} changed"),
                    };
                    section_titled(ui, ADVANCED, &title, false, open, |ui| {
                        for (name, value) in advanced {
                            row(ui, name, value);
                        }
                    });
                }
                // Services keep neither in the world file.
                // While filtering, they show if their title or one of their
                // attributes or tags matches.
                if !instance.class().is_service() {
                    let attributes = !filtering
                        || matches_filter("Attributes", filter)
                        || instance.attributes().keys().any(|a| matches_filter(a, filter));
                    let tags = !filtering
                        || matches_filter("Tags", filter)
                        || instance.tags().iter().any(|t| matches_filter(t, filter));
                    shown_any |= attributes || tags;
                    ui.add_enabled_ui(!locked, |ui| {
                        if attributes && let Some(made) = attributes_section(ui, instance, open) {
                            edit = Some(made);
                        }
                        if tags && let Some(made) = tags_section(ui, instance, open) {
                            edit = Some(made);
                        }
                    });
                }
                if !shown_any {
                    ui.add_space(8.0);
                    ui.weak(format!("No properties match \"{filter}\"."));
                }
                if instance.class() == ClassName::ScreenGui
                    && instance
                        .property("ScaleMode")
                        .and_then(Value::as_str)
                        .is_some_and(|mode| !mode.eq_ignore_ascii_case("Fixed"))
                {
                    ui.weak(
                        "Fit and Scale scale this ScreenGui evenly from DesignWidth × \
                         DesignHeight to the screen. Fit keeps the design's shape, with empty \
                         bars on other screens; Scale fills the whole screen, so things placed \
                         against an edge stay there.",
                    );
                }
                more(ui);
            });
        });
    edit
}

mod curves;
mod files;
mod groups;
mod many;
mod plugin_rows;
mod values;
#[cfg(test)]
pub use many::shared_names;
pub use many::{count_name, show_many};
pub use plugin_rows::{PluginRows, RowAction, plugin_sections};

use files::*;
use groups::*;
pub use groups::{is_advanced, is_map, sections};
use values::*;
pub(crate) use values::{color_control, enum_items};

/// The types an attribute can be, by name, with the value a new one starts at.
/// Makes the value a new attribute of one type starts at.
type NewValue = fn() -> Value;

pub const ATTRIBUTE_TYPES: [(&str, NewValue); 9] = [
    ("Boolean", || Value::Bool(false)),
    ("Number", || Value::Number(0.0)),
    ("String", || Value::String(String::new())),
    ("Vector3", || Value::Vector3(Vector3::default())),
    ("Vector2", || Value::Vector2(Vector2::default())),
    ("Color3", || Value::Color3(Color3::new(1.0, 1.0, 1.0))),
    ("UDim2", || Value::UDim2(UDim2::default())),
    ("CFrame", || Value::CFrame(CFrame::IDENTITY)),
    ("EnumItem", || {
        Value::EnumItem(EnumItem {
            enum_name: "Material".to_owned(),
            item: "Plastic".to_owned(),
        })
    }),
];

/// The Attributes section: each attribute with its control and a button
/// that removes it, then a row that adds one by name and type.
fn attributes_section(ui: &mut egui::Ui, instance: &Instance, open: Option<bool>) -> Option<Edit> {
    let mut edit = None;
    section(ui, "Attributes", open, true, |ui| {
        for (name, value) in instance.attributes() {
            // The ✕ goes by the name, so the value keeps the whole width.
            ui.horizontal(|ui| {
                field_name(ui, name);
                if widgets_remove(ui, &format!("Remove the attribute {name}")) {
                    edit = Some(Edit::Attribute(name.clone(), None));
                }
            });
            ui.push_id(name, |ui| {
                if let Some(value) = value_edit(ui, name, value.clone()) {
                    edit = Some(Edit::Attribute(name.clone(), Some(value)));
                }
            });
            ui.end_row();
        }
        // The half-typed name and the picked type, kept by egui between frames.
        let id = ui.id().with("new attribute");
        let (mut name, mut kind): (String, usize) =
            ui.data_mut(|data| data.get_temp(id)).unwrap_or_default();
        field_name(ui, "Add");
        ui.vertical(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut name)
                    .hint_text("Name")
                    .desired_width(f32::INFINITY),
            );
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt(id.with("type"))
                    .selected_text(ATTRIBUTE_TYPES[kind].0)
                    .show_ui(ui, |ui| {
                        for (index, (label, _)) in ATTRIBUTE_TYPES.iter().enumerate() {
                            ui.selectable_value(&mut kind, index, *label);
                        }
                    });
                let taken = instance.attribute(&name).is_some();
                let valid = is_attribute_name(&name) && !taken;
                let hint = if taken {
                    "It already has an attribute with this name."
                } else {
                    "Names are 1 to 100 letters, digits and underscores."
                };
                let add = ui
                    .add_enabled(valid, egui::Button::new(format!("{} Add", icons::PLUS)))
                    .on_disabled_hover_text(hint);
                if add.clicked() {
                    edit = Some(Edit::Attribute(
                        name.clone(),
                        Some(ATTRIBUTE_TYPES[kind].1()),
                    ));
                    name.clear();
                }
            });
        });
        ui.data_mut(|data| data.insert_temp(id, (name, kind)));
        ui.end_row();
    });
    edit
}

/// The Tags row: a chip for each tag (click its ✕ to remove it), and a box
/// that adds one on Enter.
fn tags_section(ui: &mut egui::Ui, instance: &Instance, open: Option<bool>) -> Option<Edit> {
    let mut edit = None;
    section(ui, "Tags", open, true, |ui| {
        field_name(ui, "Tags");
        ui.vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
                for tag in instance.tags() {
                    let chip = egui::Button::new(format!("{} {tag}  {}", icons::TAG, icons::X))
                        .corner_radius(10.0)
                        .small();
                    if ui
                        .add(chip)
                        .on_hover_text(format!("Remove the tag {tag}"))
                        .clicked()
                    {
                        edit = Some(Edit::RemoveTag(tag.clone()));
                    }
                }
            });
            let id = ui.id().with("new tag");
            let mut text: String = ui.data_mut(|data| data.get_temp(id)).unwrap_or_default();
            let response = ui.add(
                egui::TextEdit::singleline(&mut text)
                    .hint_text("Add a tag")
                    .desired_width(f32::INFINITY),
            );
            let entered = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let tag = text.trim();
            if entered && !tag.is_empty() {
                edit = Some(Edit::AddTag(tag.to_owned()));
                text.clear();
            }
            ui.data_mut(|data| data.insert_temp(id, text));
        });
        ui.end_row();
    });
    edit
}

/// A small ✕ button. True when clicked.
fn widgets_remove(ui: &mut egui::Ui, tooltip: &str) -> bool {
    ui.small_button(icons::X).on_hover_text(tooltip).clicked()
}

/// An ObjectValue's `Value`: a dropdown of every Instance in the Workspace
/// and ReplicatedStorage by its full name, and None. Returns the new
/// target, if one was picked.
fn object_control(
    ui: &mut egui::Ui,
    world: &World,
    owner: InstanceId,
    target: Option<InstanceId>,
) -> Option<Option<InstanceId>> {
    let target = target.filter(|&target| world.contains(target));
    let current = target
        .and_then(|target| world.full_name(target))
        .unwrap_or_else(|| "None".to_owned());
    let mut picked = None;
    egui::ComboBox::from_id_salt(("object value", owner))
        .selected_text(current)
        .truncate()
        .show_ui(ui, |ui| {
            if ui.selectable_label(target.is_none(), "None").clicked() {
                picked = Some(None);
            }
            let places = [
                world.workspace(),
                world.replicated_storage(),
                world.server_storage(),
            ];
            for id in places
                .into_iter()
                .flat_map(|place| world.descendants(place))
            {
                if id == owner {
                    continue;
                }
                let name = world.full_name(id).unwrap_or_default();
                if ui.selectable_label(target == Some(id), name).clicked() {
                    picked = Some(Some(id));
                }
            }
        });
    picked.filter(|&picked| picked != target)
}

/// A collapsible group with a two-column grid of names and controls.
/// `open` forces it open (while filtering) or leaves it as it was (`None`);
/// with `shown` false nothing is drawn.
fn section(
    ui: &mut egui::Ui,
    title: &str,
    open: Option<bool>,
    shown: bool,
    rows: impl FnOnce(&mut egui::Ui),
) {
    if !shown {
        return;
    }
    section_titled(ui, title, title, true, open, rows);
}

/// [`section`] with a header that can say more than its `title` (which
/// keeps its open state), and closed at first unless `default_open`.
fn section_titled(
    ui: &mut egui::Ui,
    title: &str,
    header: &str,
    default_open: bool,
    open: Option<bool>,
    rows: impl FnOnce(&mut egui::Ui),
) {
    let icon = match title {
        "Data" => icons::IDENTIFICATION_CARD,
        "Appearance" => icons::PALETTE,
        "Behavior" => icons::GEAR,
        "Physics" => icons::ATOM,
        "Part" => icons::CUBE,
        "Surface" => icons::IMAGE,
        "Map Settings" => icons::SLIDERS,
        "Terrain" => icons::MOUNTAINS,
        "Tiling" => icons::GRID_FOUR,
        "Auto Material" => icons::MAGIC_WAND,
        "UI" => icons::LAYOUT,
        "Audio" => icons::SPEAKER_HIGH,
        "Shadows" => icons::SUN,
        "Time of Day" => icons::CLOCK,
        "Sky" => icons::CLOUD_SUN,
        "Fog" => icons::CLOUD_FOG,
        "Atmosphere" => icons::WIND,
        "Clouds" => icons::CLOUD,
        "Weather" => icons::CLOUD_RAIN,
        "Outlines" => icons::PEN_NIB,
        "Color Correction" => icons::SWATCHES,
        "Lens" => icons::APERTURE,
        "Graphics Volume" => icons::BOUNDING_BOX,
        "Effect" => icons::APERTURE,
        "Post-processing" => icons::SPARKLE,
        "Light" => icons::LIGHTBULB,
        "Attributes" => icons::LIST_BULLETS,
        "Tags" => icons::TAG,
        ADVANCED => icons::SLIDERS_HORIZONTAL,
        // A plugin's rows (`plugin_rows.rs`).
        plugin if plugin.starts_with("plugin:") => icons::PUZZLE_PIECE,
        _ => icons::DOTS_THREE_OUTLINE,
    };
    // Every section's names take the same width (from the panel's), so the
    // fields line up from one section to the next. In a narrow panel the
    // values go under their names instead.
    crate::widgets::section(icon, header)
        .id_salt(title)
        .default_open(default_open)
        .forced(open)
        .show(ui, |ui| crate::responsive::form(ui, title, rows));
}

/// A property's name in the left column, after its Keep pin when it was
/// edited during Play (`pin` says whether it's kept). Returns the pin's new
/// state when it was clicked.
fn field_name_with_pin(
    ui: &mut egui::Ui,
    class: ClassName,
    name: &str,
    pin: Option<bool>,
) -> Option<bool> {
    let Some(kept) = pin else {
        described_name(ui, class, name);
        return None;
    };
    let mut clicked = None;
    let tooltip = if kept {
        "Kept: Stop copies this value into the world you're editing, as one undo step. \
         Click to let it go at Stop."
    } else {
        "Changed during Play. Click to keep this value after Stop."
    };
    crate::widgets::row_name(
        ui,
        false,
        |ui| described_name(ui, class, name),
        |ui| {
            if crate::widgets::icon_button(ui, icons::PUSH_PIN, tooltip, kept).clicked() {
                clicked = Some(!kept);
            }
        },
    );
    clicked
}

/// Whether a property holding `default` gets ↺ when it's changed: not one
/// that points at another Instance, which the game or a picker sets.
fn resettable(default: &Value) -> bool {
    !matches!(default, Value::Instance(_))
}

/// A changed property's name, marked as changed, with a ↺ after it that
/// sets it back to its class's default (one undo step, like any edit).
/// Returns whether ↺ was clicked.
fn field_name_with_reset(ui: &mut egui::Ui, class: ClassName, name: &str, default: &Value) -> bool {
    let mut clicked = false;
    let tooltip = match default_text(default) {
        Some(text) => format!("Changed. Click to reset it to the default, {text}."),
        None => "Changed. Click to reset it to the default.".to_owned(),
    };
    crate::widgets::row_name(
        ui,
        true,
        |ui| described_name(ui, class, name),
        |ui| clicked = crate::widgets::reset_button(ui, &tooltip),
    );
    clicked
}

/// A default short enough to say in a tooltip.
fn default_text(value: &Value) -> Option<String> {
    let number = |n: f64| {
        let text = format!("{n:.3}");
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    };
    match value {
        Value::Bool(on) => Some(if *on { "on" } else { "off" }.to_owned()),
        Value::Number(n) => Some(number(*n)),
        Value::String(text) if text.is_empty() => Some("empty".to_owned()),
        Value::String(text) if text.chars().count() <= 24 => Some(format!("\"{text}\"")),
        Value::Vector3(v) => Some(format!(
            "{}, {}, {}",
            number(f64::from(v.x)),
            number(f64::from(v.y)),
            number(f64::from(v.z))
        )),
        Value::EnumItem(item) => Some(item.item.clone()),
        _ => None,
    }
}

/// An overridden property's name in a placed prefab copy: bold, with a ↺
/// after it that sets it back to the prefab's value, and Apply to prefab on
/// a right-click.
fn override_name(ui: &mut egui::Ui, name: &str) -> Option<Edit> {
    let mut edit = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let palette = theme::palette(ui.ctx());
        let label = ui
            .add(
                egui::Label::new(
                    egui::RichText::new(name)
                        .strong()
                        .color(palette.text_strong),
                )
                .truncate()
                .sense(egui::Sense::click()),
            )
            .on_hover_text(
                "Overridden: this copy sets it differently from its prefab. Right-click to \
                 apply it to the prefab.",
            );
        label.context_menu(|ui| {
            if ui
                .button(theme::with_icon(icons::UPLOAD_SIMPLE, "Apply to prefab"))
                .clicked()
            {
                edit = Some(Edit::ApplyOverride(name.to_owned()));
            }
        });
        if crate::widgets::reset_button(ui, "Revert to the prefab's value") {
            edit = Some(Edit::RevertOverride(name.to_owned()));
        }
    });
    edit
}

/// A property's name, in the left column.
fn field_name(ui: &mut egui::Ui, name: &str) {
    let palette = theme::palette(ui.ctx());
    ui.add(egui::Label::new(egui::RichText::new(name).color(palette.text_secondary)).truncate());
}

/// A property's name, as [`field_name`], that says on hover what the
/// property does: the same description scripts' autocomplete and the API
/// reference give it (`axcenity_script::property_description`), looked up
/// only while hovered.
fn described_name(ui: &mut egui::Ui, class: ClassName, name: &str) {
    let palette = theme::palette(ui.ctx());
    let label = ui.add(
        egui::Label::new(egui::RichText::new(name).color(palette.text_secondary))
            .truncate()
            .show_tooltip_when_elided(false),
    );
    if label.hovered()
        && let Some(about) = description(class, name)
    {
        label.on_hover_ui(|ui| {
            ui.set_max_width(320.0);
            ui.strong(name);
            ui.label(about);
        });
    }
}

/// What `class`'s `name` does, for its hover: the Data rows' own words, or
/// the script API's description of the property.
pub fn description(class: ClassName, name: &str) -> Option<&'static str> {
    match name {
        "Name" => Some(
            "What it's called, in the Explorer and in scripts (workspace.Door). Names don't \
             have to be unique.",
        ),
        "ClassName" => Some("What kind of object it is. It can't be changed."),
        _ => axcenity_script::property_description(class, name),
    }
}

/// Types `text` into the Properties filter, as universal search does to go
/// straight to a property (`app/search.rs`). Every selection shares it.
pub fn set_filter(ctx: &egui::Context, text: &str) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("properties filter"), text.to_owned()));
}

/// Whether a property `name` (or a section titled `name`) matches what's
/// typed in the filter: letters in order, ignoring case and spaces, so
/// "walk speed" finds WalkSpeed.
pub fn matches_filter(name: &str, filter: &str) -> bool {
    let squash = |text: &str| -> String {
        text.chars()
            .filter(|c| !c.is_whitespace())
            .flat_map(char::to_lowercase)
            .collect()
    };
    squash(name).contains(&squash(filter))
}

/// A note above the properties, such as during Play.
fn notice(ui: &mut egui::Ui, icon: &str, text: &str) {
    let palette = theme::palette(ui.ctx());
    egui::Frame::new()
        .fill(palette.surface)
        .stroke(egui::Stroke::new(1.0_f32, palette.border))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.label(egui::RichText::new(icon).color(palette.text_weak));
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(text)
                            .size(theme::type_scale::SMALL)
                            .color(palette.text_secondary),
                    )
                    .wrap(),
                );
            });
        });
    ui.add_space(4.0);
}

/// Rows for the properties of `instance` named in `names` (those it has),
/// drawn as Properties draws them (a name that says on hover what it does,
/// ↺ back to the default, and the same controls), in a form keyed by
/// `salt`. A start and end pair whose curve is set is greyed out, as in
/// Properties. Returns the edit made this frame. The Effect tab's module
/// stack draws an emitter's modules with it (roadmap R17.3).
pub fn property_rows(
    ui: &mut egui::Ui,
    instance: &Instance,
    assets: &Assets,
    names: &[&'static str],
    salt: &str,
) -> Option<Edit> {
    let class = instance.class();
    let defaults: HashMap<&str, Value> = class.default_properties().into_iter().collect();
    let locked = instance.property("Locked") == Some(&Value::Bool(true));
    let mut edit = None;
    crate::responsive::form(ui, salt, |ui| {
        for &name in names {
            let Some(value) = instance.property(name).cloned() else {
                continue;
            };
            match defaults
                .get(name)
                .filter(|default| resettable(default) && **default != value)
            {
                Some(default) if !locked => {
                    if field_name_with_reset(ui, class, name, default) {
                        edit = Some(Edit::Property(name, default.clone()));
                    }
                }
                _ => described_name(ui, class, name),
            }
            let replaced = class == ClassName::ParticleEmitter
                && groups::replacing_curve(name)
                    .and_then(|curve| instance.property(curve))
                    .is_some_and(|curve| match curve {
                        Value::NumberSequence(sequence) => !sequence.is_empty(),
                        Value::ColorSequence(sequence) => !sequence.is_empty(),
                        _ => false,
                    });
            let enabled = (!locked || name == "Locked") && !replaced;
            let made = ui
                .add_enabled_ui(enabled, |ui| match value {
                    Value::Instance(_) | Value::PhysicalProperties(_) => {
                        ui.weak("(in Properties)");
                        None
                    }
                    value => value_control(ui, name, value, assets),
                })
                .inner;
            if made.is_some() {
                edit = made;
            }
            ui.end_row();
        }
    });
    edit
}

/// Draws the control for one property and returns the edit if the user
/// changed it this frame.
fn value_control(
    ui: &mut egui::Ui,
    name: &'static str,
    value: Value,
    assets: &Assets,
) -> Option<Edit> {
    match value {
        Value::String(path) if is_map(name) => map_control(ui, name, &path, assets),
        Value::String(path) if name == "SoundId" => sound_control(ui, &path, assets),
        Value::String(path) if name == "Image" => image_control(ui, &path),
        Value::String(path) if name == "StateMachine" => controller_control(ui, &path, assets),
        Value::String(path) if name == "Clip" => clip_control(ui, &path, assets),
        Value::String(path) if name == "BehaviorTree" => behavior_tree_control(ui, &path),
        Value::String(path) if name == "SourceFile" => source_file_control(ui, &path),
        Value::String(path) if name == "SkyImage" => sky_control(ui, &path, assets),
        Value::String(path) if matches!(name, "LookupTable" | "Palette") => {
            grading_control(ui, name, &path, assets)
        }
        // A model named by its asset ID shows the file it names.
        Value::String(mesh_id) if name == "MeshId" && is_asset_reference(&mesh_id) => {
            let shown = assets
                .asset_path(&mesh_id)
                .unwrap_or_else(|| format!("(no file has the ID {mesh_id})"));
            let hover =
                format!("{mesh_id}\n(the model's asset ID: it follows the file when it moves)");
            ui.add_enabled(false, egui::TextEdit::singleline(&mut shown.as_str()))
                .on_disabled_hover_text(hover);
            None
        }
        // Picked by the editor; the Terrain Studio says where it is.
        Value::String(path) if name == "DataFolder" => {
            ui.label(if path.is_empty() {
                "(chosen on save)"
            } else {
                path.as_str()
            })
            .on_hover_text("Where this terrain's heights and paint are saved, in the project.");
            None
        }
        value => value_edit(ui, name, value).map(|value| Edit::Property(name, value)),
    }
}

#[cfg(test)]
mod tests;
