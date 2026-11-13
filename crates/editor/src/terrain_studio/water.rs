//! The Water section (LE4): floods the terrain's hollows up to a level, with
//! the renderer's waves, foam and reflections (`docs/terrain.md`).

use super::*;

impl TerrainStudio {
    /// The terrain's water: on or off, its level, color and waves.
    pub(super) fn water_section(
        &mut self,
        ui: &mut egui::Ui,
        world: &World,
        terrain: InstanceId,
        actions: &mut Vec<TerrainAction>,
    ) {
        let Some(instance) = world.get(terrain) else {
            return;
        };
        let number = |name: &str, otherwise: f64| {
            instance
                .property(name)
                .and_then(Value::as_number)
                .unwrap_or(otherwise)
        };
        let vector = |name: &str| match instance.property(name) {
            Some(Value::Vector3(v)) => Some(*v),
            _ => None,
        };
        let mut set = |name: &'static str, value: Value| {
            actions.push(TerrainAction::SetTerrain(terrain, name, value));
        };
        section(ui, "Water", |ui| {
            let mut on = instance.property("Water").and_then(Value::as_bool) == Some(true);
            if ui
                .checkbox(&mut on, "Fill the hollows with water")
                .on_hover_text("Seas, lakes and rivers: ground below the water level is under water, with waves, foam and reflections.")
                .changed()
            {
                set("Water", Value::Bool(on));
            }
            if !on {
                return;
            }
            // The level slides over the terrain's height range.
            let (bottom, top) = match (vector("Position"), vector("Size")) {
                (Some(position), Some(size)) => {
                    (position.y as f64, position.y as f64 + size.y as f64)
                }
                _ => (-64.0, 64.0),
            };
            egui::Grid::new(("terrain water", terrain))
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    field(ui, "Level");
                    let mut level = number("WaterLevel", 0.0);
                    if ui
                        .add(egui::Slider::new(&mut level, bottom..=top).suffix(" Y"))
                        .on_hover_text("The world height of the water's surface at rest.")
                        .changed()
                    {
                        set("WaterLevel", Value::Number(level));
                    }
                    ui.end_row();
                    field(ui, "Color");
                    let color = instance
                        .property("WaterColor")
                        .and_then(Value::as_color3)
                        .unwrap_or(Color3::new(0.02, 0.16, 0.22));
                    let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
                    let mut rgb = [byte(color.r), byte(color.g), byte(color.b)];
                    if crate::color_palette::edit_srgb(ui, &mut rgb)
                        .on_hover_text("The color deep water shows.")
                        .changed()
                    {
                        let [r, g, b] = rgb.map(|c| c as f32 / 255.0);
                        set("WaterColor", Value::Color3(Color3::new(r, g, b)));
                    }
                    ui.end_row();
                    for (name, label, otherwise, range, suffix, tip) in [
                        ("WaterClarity", "Clarity", 4.0, 0.1..=64.0, " studs", "How far light crosses the water before what's under it fades. Bigger is clearer."),
                        ("WaveHeight", "Wave height", 0.25, 0.0..=4.0, " studs", "How high the biggest wave rises above the surface at rest."),
                        ("WaveLength", "Wave length", 12.0, 1.0..=128.0, " studs", "From one crest of the biggest wave to the next."),
                        ("WaveSpeed", "Wave speed", 1.0, 0.0..=3.0, "×", "1 rolls like real waves of that length; 0 stills them."),
                        ("WaveDirection", "Wave direction", 0.0, -180.0..=180.0, "°", "0 rolls toward +X, 90 toward +Z."),
                    ] {
                        field(ui, label);
                        let mut value = number(name, otherwise);
                        if ui
                            .add(egui::Slider::new(&mut value, range).suffix(suffix))
                            .on_hover_text(tip)
                            .changed()
                        {
                            set(name, Value::Number(value));
                        }
                        ui.end_row();
                    }
                });
        });
    }
}
