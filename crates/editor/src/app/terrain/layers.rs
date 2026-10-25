use super::*;

/// A new TerrainLayer like `preset`, outside the tree, named so no child
/// of `terrain` has its name.
pub(super) fn new_layer(
    world: &mut World,
    terrain: InstanceId,
    preset: &LayerPreset,
    height: f32,
) -> Option<InstanceId> {
    let layer = world.create(ClassName::TerrainLayer).ok()?;
    for (property, value) in preset.properties(height) {
        world
            .set_property(layer, property, value)
            .expect("presets set TerrainLayer properties");
    }
    let name = tree::unique_name(world, terrain, preset.name);
    world.set_name(layer, name).expect("just made");
    Some(layer)
}

/// The property edits that put the texture at `path` in a layer's maps.
pub(super) fn layer_map_edits(roles: &[axcenity_assets::TextureRole], path: &str) -> Vec<Edit> {
    use axcenity_assets::MapSlot;
    let file = || Value::String(path.to_owned());
    let channel = |role: &axcenity_assets::TextureRole| {
        Value::TextureChannel(match role.channel {
            axcenity_assets::Channel::Red => TextureChannel::Red,
            axcenity_assets::Channel::Green => TextureChannel::Green,
            axcenity_assets::Channel::Blue => TextureChannel::Blue,
            axcenity_assets::Channel::Alpha => TextureChannel::Alpha,
        })
    };
    let mut edits = Vec::new();
    for role in roles {
        match role.slot {
            MapSlot::BaseColor => {
                edits.push(Edit::Property("ColorMap", file()));
                // The map is the color: no tint over it.
                edits.push(Edit::Property(
                    "Color",
                    Value::Color3(Color3::new(1.0, 1.0, 1.0)),
                ));
            }
            MapSlot::Normal => {
                edits.push(Edit::Property("NormalMap", file()));
                edits.push(Edit::Property(
                    "NormalMapDirectX",
                    Value::Bool(role.flip_green),
                ));
            }
            MapSlot::Roughness => {
                edits.push(Edit::Property("RoughnessMap", file()));
                edits.push(Edit::Property("RoughnessChannel", channel(role)));
                edits.push(Edit::Property("InvertRoughness", Value::Bool(role.invert)));
            }
            MapSlot::Metalness => {
                edits.push(Edit::Property("MetalnessMap", file()));
                edits.push(Edit::Property("MetalnessChannel", channel(role)));
            }
            MapSlot::Occlusion => {
                edits.push(Edit::Property("AmbientOcclusionMap", file()));
                edits.push(Edit::Property("AmbientOcclusionChannel", channel(role)));
            }
            MapSlot::Height => edits.push(Edit::Property("HeightMap", file())),
            // Terrain layers have no glow, see-through or specular maps.
            MapSlot::Emissive | MapSlot::Opacity | MapSlot::Specular => {}
        }
    }
    edits
}

impl EditorApp {
    /// Adds layers to the end of `terrain`'s, as one undo step. Layers past
    /// the most a terrain draws aren't added.
    pub(super) fn add_terrain_layers(
        &mut self,
        terrain: InstanceId,
        presets: &[LayerPreset],
        label: String,
    ) {
        let Some(instance) = self.world.get(terrain) else {
            return;
        };
        let height = instance
            .property("Size")
            .and_then(Value::as_vector3)
            .map_or(128.0, |size| size.y);
        let room = MAX_TERRAIN_LAYERS.saturating_sub(materials::layers(&self.world, terrain).len());
        if presets.len() > room {
            self.output.warning(format!(
                "A terrain draws up to {MAX_TERRAIN_LAYERS} layers, so only {room} more {} added.",
                if room == 1 { "was" } else { "were" }
            ));
        }
        let mut moves = Vec::new();
        for preset in presets.iter().take(room) {
            let Some(layer) = new_layer(&mut self.world, terrain, preset, height) else {
                continue;
            };
            let index = self.world.get(terrain).map_or(0, |t| t.children().len());
            let place = Place {
                parent: terrain,
                index,
            };
            if history::put(&mut self.world, layer, Some(place)).is_ok() {
                moves.push(Move {
                    id: layer,
                    before: None,
                    after: Some(place),
                });
            }
        }
        if !moves.is_empty() {
            self.history.record_tree(&mut self.world, label, moves);
        }
    }

    /// Swaps a layer with the one before (`up`) or after it.
    pub(super) fn move_terrain_layer(&mut self, layer: InstanceId, up: bool) {
        let Some(place) = history::place_of(&self.world, layer) else {
            return;
        };
        let Some(siblings) = self.world.get(place.parent).map(|p| p.children().to_vec()) else {
            return;
        };
        let is_layer = |id: &InstanceId| {
            self.world.get(*id).map(|i| i.class()) == Some(ClassName::TerrainLayer)
        };
        // Taking the layer out first moves later siblings back one, so
        // going down, the next layer's index is where it ends up after it.
        let index = if up {
            siblings[..place.index].iter().rposition(is_layer)
        } else {
            siblings[place.index + 1..]
                .iter()
                .position(is_layer)
                .map(|offset| place.index + 1 + offset)
        };
        let Some(index) = index else {
            return;
        };
        let name = self
            .world
            .get(layer)
            .map_or(String::new(), |l| l.name().to_string());
        let label = format!("Move {name} {}", if up { "Up" } else { "Down" });
        self.place(
            layer,
            Some(Place {
                parent: place.parent,
                index,
            }),
            label,
        );
    }

    /// Picks texture files and puts each in the layer's maps its name says
    /// (BaseColor, Normal, ORM…), as one undo step.
    pub(super) fn import_texture_set(&mut self, layer: InstanceId) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let Some(files) = rfd::FileDialog::new()
            .set_title("Choose a material's textures")
            .add_filter("Texture", &axcenity_assets::TEXTURE_EXTENSIONS)
            .set_directory(&folder)
            .pick_files()
        else {
            return;
        };
        let mut edits = Vec::new();
        let mut unknown = Vec::new();
        for file in files {
            let file_name = file
                .file_name()
                .map_or(String::new(), |name| name.to_string_lossy().into_owned());
            let roles = axcenity_assets::texture_roles(&file_name);
            if roles.is_empty() {
                unknown.push(file_name);
                continue;
            }
            let path = match axcenity_assets::import_texture(
                &folder,
                &file,
                axcenity_assets::PROJECT_TEXTURES_FOLDER,
            ) {
                Ok(path) => path,
                Err(err) => {
                    self.output.error(format!(
                        "Could not use `{}`: {}",
                        file.display(),
                        super::super::files::chain(&err)
                    ));
                    continue;
                }
            };
            edits.extend(layer_map_edits(&roles, &path));
        }
        if !unknown.is_empty() {
            self.output.warning(format!(
                "Skipped {}: their names don't say which map they are (such as `Rock_Normal.png`).",
                unknown.join(", ")
            ));
        }
        if edits.is_empty() {
            return;
        }
        let name = self
            .world
            .get(layer)
            .map_or(String::new(), |l| l.name().to_string());
        self.change(
            layer,
            Field::Property("ColorMap"),
            format!("Import Textures into {name}"),
            edits,
        );
        self.history.seal();
    }

    /// Copies the selected SurfaceAppearance's maps into the layer.
    pub(super) fn copy_surface_appearance(&mut self, layer: InstanceId) {
        let source = self.selection.iter().copied().find(|&id| {
            self.world.get(id).map(|i| i.class()) == Some(ClassName::SurfaceAppearance)
        });
        let Some(source) = source.and_then(|id| self.world.get(id)) else {
            self.output.info(
                "Select a SurfaceAppearance in the Explorer first (Ctrl+click it to keep the layer \
                 open), then copy its maps.",
            );
            return;
        };
        const COPIED: [&str; 12] = [
            "Color",
            "ColorMap",
            "NormalMap",
            "RoughnessMap",
            "MetalnessMap",
            "AmbientOcclusionMap",
            "NormalStrength",
            "NormalMapDirectX",
            "RoughnessChannel",
            "InvertRoughness",
            "MetalnessChannel",
            "AmbientOcclusionChannel",
        ];
        let edits: Vec<Edit> = COPIED
            .into_iter()
            .filter_map(|name| Some(Edit::Property(name, source.property(name)?.clone())))
            .collect();
        let name = self
            .world
            .get(layer)
            .map_or(String::new(), |l| l.name().to_string());
        self.change(
            layer,
            Field::Property("ColorMap"),
            format!("Copy Material into {name}"),
            edits,
        );
        self.history.seal();
    }
}
