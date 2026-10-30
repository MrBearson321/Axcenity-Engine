use super::*;

/// Draws the blue box around the selected part, Studio-style: the edges of
/// the sides facing the camera, where no other part is in front of them.
/// Draws nothing if `selected` isn't a part.
pub(super) fn paint_selection_box(
    ui: &egui::Ui,
    rect: egui::Rect,
    screen: &Screen,
    world: &World,
    selected: InstanceId,
    eye: Vec3,
) {
    let Some(lines) = axcenity_scene::visible_box_edges(world, selected, eye) else {
        return;
    };
    let painter = ui.painter_at(rect);
    // A soft glow under the line keeps it readable on any background.
    let glow = egui::Stroke::new(SELECTION_WIDTH + 3.0, SELECTION_COLOR.gamma_multiply(0.25));
    let stroke = egui::Stroke::new(SELECTION_WIDTH, SELECTION_COLOR);
    let lines: Vec<_> = lines
        .into_iter()
        .filter_map(|(a, b)| screen.line(a, b))
        .collect();
    for line in &lines {
        painter.line_segment(*line, glow);
    }
    for line in lines {
        painter.line_segment(line, stroke);
    }
}

/// The light whose icon is under `pointer`, the nearest if several are.
pub(super) fn light_at(
    lights: &[(LightMarker, egui::Pos2)],
    pointer: egui::Pos2,
) -> Option<InstanceId> {
    lights
        .iter()
        .filter(|(_, at)| at.distance(pointer) <= LIGHT_ICON_RADIUS + 2.0)
        .min_by(|(_, a), (_, b)| a.distance(pointer).total_cmp(&b.distance(pointer)))
        .map(|(marker, _)| marker.id)
}

pub(super) fn light_color(marker: &LightMarker) -> egui::Color32 {
    let [r, g, b] = marker
        .color
        .map(|channel| (channel.clamp(0.0, 1.0) * 255.0) as u8);
    egui::Color32::from_rgb(r, g, b)
}

/// Draws each free-standing light's or emitter's icon (a bulb for a
/// PointLight, a flashlight for a SpotLight, a frame for a RectLight,
/// sparkles for a ParticleEmitter) in its color.
pub(super) fn paint_lights(
    ui: &egui::Ui,
    rect: egui::Rect,
    lights: &[(LightMarker, egui::Pos2)],
    selected: Option<InstanceId>,
    pointer: Option<egui::Pos2>,
) {
    let painter = ui.painter_at(rect);
    let hovered = pointer.and_then(|pointer| light_at(lights, pointer));
    for (marker, at) in lights {
        // Grows a little under the mouse.
        let grow = ui.ctx().animate_bool_with_time(
            egui::Id::new(("light icon", marker.id)),
            hovered == Some(marker.id),
            0.12,
        );
        let radius = LIGHT_ICON_RADIUS * (1.0 + 0.15 * grow);
        let (fill, tint) = if marker.enabled {
            (egui::Color32::from_black_alpha(170), light_color(marker))
        } else {
            // Switched off: grey.
            (
                egui::Color32::from_black_alpha(120),
                egui::Color32::from_gray(150),
            )
        };
        painter.circle_filled(*at, radius, fill);
        let ring = if Some(marker.id) == selected {
            egui::Stroke::new(2.0_f32, SELECTION_COLOR)
        } else {
            egui::Stroke::new(1.0 + grow, tint.gamma_multiply(0.8))
        };
        painter.circle_stroke(*at, radius, ring);
        let icon = match marker.kind {
            MarkerKind::Point => icons::LIGHTBULB,
            MarkerKind::Spot => icons::FLASHLIGHT,
            MarkerKind::Rect => icons::FRAME_CORNERS,
            MarkerKind::Emitter => icons::SPARKLE,
        };
        painter.text(
            *at,
            egui::Align2::CENTER_CENTER,
            icon,
            egui::FontId::new(radius * 1.15, theme::filled()),
            tint,
        );
    }
}

/// A light's reach in its color: three rings for a PointLight's range;
/// for a RectLight its rectangle, the box it lights out of its front and
/// an arrow the way it faces; or for a SpotLight the cone it lights
/// (see-through sides, edges and the far rim), the beam's middle as an
/// arrow, and where that beam lands on a part, as a ring the size of the
/// spot. `chosen` (the light or its part is
/// selected) draws it stronger; View > Light Cones draws the others faintly.
pub(super) fn paint_light_reach(
    painter: &egui::Painter,
    screen: &Screen,
    world: &World,
    marker: &LightMarker,
    chosen: bool,
) {
    const SEGMENTS: usize = 48;
    let strength = if chosen { 0.85 } else { 0.4 };
    let color = light_color(marker);
    let stroke = egui::Stroke::new(1.5_f32, color.gamma_multiply(strength));
    let faint = egui::Stroke::new(1.0_f32, color.gamma_multiply(strength * 0.5));
    let range = marker.range.max(0.1);
    let circle = |center: Vec3, a: Vec3, b: Vec3, radius: f32| -> Vec<Vec3> {
        (0..=SEGMENTS)
            .map(|i| {
                let turn = i as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
                center + (a * turn.cos() + b * turn.sin()) * radius
            })
            .collect()
    };
    let outline = |points: &[Vec3], stroke: egui::Stroke| {
        for pair in points.windows(2) {
            if let Some(line) = screen.line(pair[0], pair[1]) {
                painter.line_segment(line, stroke);
            }
        }
    };
    if let Some((facing, right, width, height)) = marker.rect {
        let up = facing.cross(right);
        let half = (right * width.max(0.0) * 0.5, up * height.max(0.0) * 0.5);
        let face = |center: Vec3, grow: f32| {
            let (a, b) = (half.0 * grow, half.1 * grow);
            vec![
                center - a - b,
                center + a - b,
                center + a + b,
                center - a + b,
                center - a - b,
            ]
        };
        let near = face(marker.position, 1.0);
        // How far out of its front it reaches, a little wider there.
        let far = face(
            marker.position + facing * range,
            1.0 + range / width.max(height).max(1.0),
        );
        if let Some(points) = near
            .iter()
            .map(|&p| screen.point(p))
            .collect::<Option<Vec<_>>>()
        {
            let fill = color.gamma_multiply(if chosen { 0.25 } else { 0.1 });
            painter.add(egui::Shape::convex_polygon(
                points[..4].to_vec(),
                fill,
                egui::Stroke::NONE,
            ));
        }
        outline(&near, stroke);
        outline(&far, faint);
        for k in 0..4 {
            if let Some(line) = screen.line(near[k], far[k]) {
                painter.line_segment(line, faint);
            }
        }
        let tip = marker.position + facing * (range * 0.25).clamp(1.0, 4.0);
        if let (Some(from), Some(to)) = (screen.point(marker.position), screen.point(tip)) {
            painter.arrow(
                from,
                to - from,
                egui::Stroke::new(
                    if chosen { 2.0_f32 } else { 1.0 },
                    color.gamma_multiply(strength),
                ),
            );
        }
        return;
    }
    let Some((direction, angle)) = marker.cone else {
        outline(&circle(marker.position, Vec3::X, Vec3::Y, range), stroke);
        outline(&circle(marker.position, Vec3::X, Vec3::Z, range), stroke);
        outline(&circle(marker.position, Vec3::Y, Vec3::Z, range), stroke);
        return;
    };

    let half = (angle.clamp(1.0, 179.0) * 0.5).to_radians();
    let (side, up) = direction.any_orthonormal_pair();
    let end = marker.position + direction * range * half.cos();
    let radius = range * half.sin();
    let rim = circle(end, side, up, radius);
    // The sides, as thin triangles from the light to the rim. Each one is
    // convex, which is all egui fills.
    let fill = color.gamma_multiply(if chosen { 0.12 } else { 0.05 });
    if let Some(apex) = screen.point(marker.position) {
        for pair in rim.windows(2) {
            if let (Some(a), Some(b)) = (screen.point(pair[0]), screen.point(pair[1])) {
                painter.add(egui::Shape::convex_polygon(
                    vec![apex, a, b],
                    fill,
                    egui::Stroke::NONE,
                ));
            }
        }
    }
    outline(&rim, stroke);
    // Halfway out, where the light is noticeably dimmer.
    let middle = marker.position + direction * range * half.cos() * 0.5;
    outline(&circle(middle, side, up, radius * 0.5), faint);
    for turn in 0..8 {
        let around = turn as f32 / 8.0 * std::f32::consts::TAU;
        let edge = end + (side * around.cos() + up * around.sin()) * radius;
        if let Some(line) = screen.line(marker.position, edge) {
            painter.line_segment(line, if turn % 2 == 0 { stroke } else { faint });
        }
    }

    // The beam's middle, out to where it first meets a part (not its own).
    let ray = axcenity_scene::Ray {
        origin: marker.position,
        direction,
    };
    let own: Vec<InstanceId> = marker.part.into_iter().collect();
    let hit =
        axcenity_scene::pick_hit_skipping(world, ray, &own).filter(|hit| hit.distance <= range);
    let beam_end = ray.at(hit.map_or(range, |hit| hit.distance));
    if let (Some(from), Some(to)) = (screen.point(marker.position), screen.point(beam_end)) {
        let width = if chosen { 2.0_f32 } else { 1.0 };
        painter.arrow(
            from,
            to - from,
            egui::Stroke::new(width, color.gamma_multiply(strength)),
        );
    }
    if let Some(hit) = hit {
        // The spot on the surface: the cone's width at that distance, laid
        // flat on the surface, lifted a hair so it isn't hidden in it.
        let spot = hit.distance * half.tan();
        let (a, b) = hit.normal.any_orthonormal_pair();
        let lifted = beam_end + hit.normal * 0.02;
        outline(&circle(lifted, a, b, spot), stroke);
        if let Some(at) = screen.point(lifted) {
            painter.circle_filled(at, 3.0, color.gamma_multiply(strength));
        }
    }
}

/// The brush's rim (and the inner edge of its fade, fainter) laid on the
/// ground around `center`, and a dot in the middle.
pub(super) fn paint_terrain_brush(
    painter: &egui::Painter,
    screen: &Screen,
    world: &World,
    brush: TerrainBrush,
    center: Vec3,
) {
    let ground = world.get(brush.terrain).and_then(|instance| {
        Some((
            axcenity_terrain::Placement::of(instance)?,
            world.terrain(brush.terrain)?.clone(),
        ))
    });
    let on_ground = |x: f32, z: f32| {
        let y = ground
            .as_ref()
            .and_then(|(placement, data)| placement.ground_y(data, x, z))
            .unwrap_or(center.y);
        // A little above, so the line isn't hidden in the ground.
        Vec3::new(x, y + 0.05, z)
    };
    let outline = |radius: f32| -> Vec<Vec3> {
        const SEGMENTS: usize = 72;
        (0..=SEGMENTS)
            .map(|i| {
                let t = i as f32 / SEGMENTS as f32;
                let (x, z) = if brush.square {
                    // Round the square's edge, a quarter per side.
                    let side = (t * 4.0).floor().min(3.0);
                    let along = t * 4.0 - side;
                    let s = along * 2.0 - 1.0;
                    match side as u8 {
                        0 => (s, -1.0),
                        1 => (1.0, s),
                        2 => (-s, 1.0),
                        _ => (-1.0, -s),
                    }
                } else {
                    let angle = t * std::f32::consts::TAU;
                    (angle.cos(), angle.sin())
                };
                on_ground(center.x + x * radius, center.z + z * radius)
            })
            .collect()
    };
    let draw = |points: Vec<Vec3>, stroke: egui::Stroke| {
        for pair in points.windows(2) {
            if let Some(line) = screen.line(pair[0], pair[1]) {
                painter.line_segment(line, stroke);
            }
        }
    };
    // A dark line under a bright one, so it reads on any ground.
    let shadow = egui::Stroke::new(3.5_f32, egui::Color32::from_black_alpha(110));
    draw(outline(brush.radius), shadow);
    draw(
        outline(brush.radius),
        egui::Stroke::new(1.6_f32, brush.color),
    );
    let inner = brush.radius * (1.0 - brush.falloff.clamp(0.0, 1.0));
    if inner > 0.05 && brush.falloff > 0.02 {
        draw(
            outline(inner),
            egui::Stroke::new(1.0_f32, brush.color.gamma_multiply(0.55)),
        );
    }
    if let Some(at) = screen.point(on_ground(center.x, center.z)) {
        painter.circle_filled(at, 3.0, brush.color);
    }
}

pub(super) fn paint_debug_overlays(
    painter: &egui::Painter,
    screen: &Screen,
    world: &World,
    assets: &Assets,
    overlays: OverlayToggles,
    debug_rays: &[DebugRay],
    eye: Vec3,
) {
    let draw_edges = |edges: &[(Vec3, Vec3)], stroke: egui::Stroke| {
        for &(a, b) in edges {
            if let Some(line) = screen.line(a, b) {
                painter.line_segment(line, stroke);
            }
        }
    };
    if overlays.colliders {
        for outline in axcenity_physics::debug_colliders(world, assets) {
            if outline.chunk && (v3(outline.center) - eye).length() > TERRAIN_OVERLAY_DISTANCE {
                continue;
            }
            let color = if outline.can_collide {
                COLLIDER_COLOR
            } else {
                SENSOR_COLOR
            };
            let edges: Vec<(Vec3, Vec3)> =
                outline.edges.iter().map(|&(a, b)| (v3(a), v3(b))).collect();
            draw_edges(&edges, egui::Stroke::new(1.0_f32, color));
        }
    }
    if overlays.bounds {
        for id in world.descendants(world.workspace()) {
            let Some(instance) = world.get(id) else {
                continue;
            };
            let Some(edges) = bounds_edges(instance) else {
                continue;
            };
            draw_edges(&edges, egui::Stroke::new(1.0_f32, BOUNDS_COLOR));
        }
    }
    if overlays.raycasts {
        for ray in debug_rays {
            let origin = v3(ray.origin);
            let far = origin + v3(ray.direction);
            let end = ray.hit.map(|hit| v3(hit.position)).unwrap_or(far);
            if let Some(line) = screen.line(origin, end) {
                painter.line_segment(line, egui::Stroke::new(2.0_f32, RAY_COLOR));
            }
            if let Some(hit) = ray.hit
                && let Some(at) = screen.point(v3(hit.position))
            {
                painter.circle_filled(at, 4.0, RAY_HIT_COLOR);
                painter.circle_stroke(at, 6.0, egui::Stroke::new(1.5_f32, RAY_HIT_COLOR));
            }
        }
    }
    if overlays.emitters {
        for id in world.descendants(world.root()) {
            let Some(instance) = world.get(id) else {
                continue;
            };
            if instance.class() != ClassName::Sound
                || instance.property("Spatial").and_then(Value::as_bool) != Some(true)
            {
                continue;
            }
            let at = v3(axcenity_audio::source_position(world, id));
            let Some(pos) = screen.point(at) else {
                continue;
            };
            painter.circle_filled(
                pos,
                EMITTER_ICON_RADIUS,
                egui::Color32::from_black_alpha(160),
            );
            painter.circle_stroke(
                pos,
                EMITTER_ICON_RADIUS,
                egui::Stroke::new(1.5_f32, EMITTER_COLOR),
            );
            painter.text(
                pos,
                egui::Align2::CENTER_CENTER,
                icons::SPEAKER_HIGH,
                egui::FontId::new(EMITTER_ICON_RADIUS * 1.15, theme::filled()),
                EMITTER_COLOR,
            );
        }
    }
}

/// The culling overlay: each item's bounding sphere, green where it was
/// drawn and red where it was culled, and the edges of the `frozen` view
/// culling is held at (seen with `aspect`). Spheres are drawn as circles
/// facing the `camera`.
pub(super) fn paint_culling(
    painter: &egui::Painter,
    screen: &Screen,
    bounds: &[axcenity_render::CullingBounds],
    frozen: axcenity_render::Camera,
    camera: axcenity_render::Camera,
    aspect: f32,
) {
    let forward = (camera.look_at - camera.position).normalize_or(Vec3::NEG_Z);
    let right = forward.cross(Vec3::Y).normalize_or(Vec3::X);
    for sphere in bounds {
        let (Some(center), Some(edge)) = (
            screen.point(sphere.center),
            screen.point(sphere.center + right * sphere.radius),
        ) else {
            continue;
        };
        let color = if sphere.drawn {
            CULL_DRAWN_COLOR
        } else {
            CULL_CULLED_COLOR
        };
        painter.circle_stroke(
            center,
            center.distance(edge),
            egui::Stroke::new(1.0_f32, color),
        );
    }

    // The frozen view: lines from its eye along the corners of the
    // picture, and the picture's frame where they end.
    let eye = frozen.position;
    let forward = (frozen.look_at - eye).normalize_or(Vec3::NEG_Z);
    let right = forward.cross(Vec3::Y).normalize_or(Vec3::X);
    let up = right.cross(forward);
    let tall = (frozen.fov_y_degrees.to_radians() * 0.5).tan() * FROZEN_VIEW_LENGTH;
    let wide = tall * aspect;
    let middle = eye + forward * FROZEN_VIEW_LENGTH;
    let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .map(|(x, y)| middle + right * (wide * x) + up * (tall * y));
    let stroke = egui::Stroke::new(1.5_f32, FROZEN_VIEW_COLOR);
    for (index, &corner) in corners.iter().enumerate() {
        let next = corners[(index + 1) % corners.len()];
        for (a, b) in [(eye, corner), (corner, next)] {
            if let Some(line) = screen.line(a, b) {
                painter.line_segment(line, stroke);
            }
        }
    }
}

/// Fills the navigation mesh's cells near the camera, one colour per
/// connected region, so ground that can't be reached from elsewhere stands
/// out, and draws the walkable area's edges.
pub(super) fn paint_nav_mesh(
    painter: &egui::Painter,
    screen: &Screen,
    mesh: &axcenity_nav::NavMesh,
    eye: Vec3,
) {
    // Raised a little so the overlay reads on top of the ground it marks.
    let lift = Vec3::new(0.0, 0.05, 0.0);
    let near = |point: Vec3, distance: f32| (point - eye).length() <= distance;
    for (index, cell) in mesh.cells().iter().enumerate() {
        let corners = mesh.corners(index).map(|corner| v3(corner) + lift);
        let centre = (corners[0] + corners[2]) * 0.5;
        if !near(centre, NAV_FILL_DISTANCE) {
            continue;
        }
        // Inset so the cells read as a grid.
        let inset = corners.map(|corner| corner + (centre - corner) * 0.08);
        let points: Option<Vec<egui::Pos2>> = inset.iter().map(|&c| screen.point(c)).collect();
        if let Some(points) = points {
            painter.add(egui::Shape::convex_polygon(
                points,
                nav_region_color(cell.region).gamma_multiply(0.35),
                egui::Stroke::NONE,
            ));
        }
    }
    for (a, b) in mesh.outline() {
        let (a, b) = (v3(a) + lift, v3(b) + lift);
        if !near((a + b) * 0.5, TERRAIN_OVERLAY_DISTANCE) {
            continue;
        }
        if let Some(line) = screen.line(a, b) {
            painter.line_segment(line, egui::Stroke::new(1.5_f32, NAV_EDGE_COLOR));
        }
    }
}

/// Draws what's left of each agent's path: a line from its feet through
/// the corners ahead, a dot on each corner, and a ring at the end. It only
/// reads the paths, so drawing them never changes a route.
pub(crate) fn paint_agent_paths(
    painter: &egui::Painter,
    screen: &Screen,
    paths: &[(InstanceId, Vec<axcenity_core::Vector3>)],
) {
    // Raised a little so the line reads on top of the ground it follows.
    let lift = Vec3::new(0.0, 0.1, 0.0);
    let stroke = egui::Stroke::new(2.5_f32, AGENT_PATH_COLOR);
    for (_, path) in paths {
        let points: Vec<Vec3> = path.iter().map(|&p| v3(p) + lift).collect();
        for pair in points.windows(2) {
            if let Some(line) = screen.line(pair[0], pair[1]) {
                painter.line_segment(line, stroke);
            }
        }
        let corners = points.len().saturating_sub(1);
        for (i, &corner) in points.iter().enumerate().skip(1) {
            let Some(at) = screen.point(corner) else {
                continue;
            };
            if i == corners {
                painter.circle_stroke(at, 7.0, stroke);
            } else {
                painter.circle_filled(at, 3.5, AGENT_PATH_COLOR);
            }
        }
    }
}

/// A distinct hue for each region: the golden angle keeps neighbouring
/// numbers far apart on the colour wheel.
fn nav_region_color(region: u32) -> egui::Color32 {
    let hue = (0.45 + region as f32 * 0.618_034).fract();
    egui::ecolor::Hsva::new(hue, 0.65, 0.95, 1.0).into()
}

pub(super) fn bounds_edges(instance: &Instance) -> Option<[(Vec3, Vec3); 12]> {
    if let Some(pose) = Pose::of(instance) {
        return Some(angular::box_edges(pose));
    }
    if instance.class() != ClassName::Terrain {
        return None;
    }
    let vector = |name| instance.property(name).and_then(Value::as_vector3);
    let position = vector("Position")?;
    let size = vector("Size")?;
    Some(angular::box_edges(Pose {
        position: Vec3::new(position.x, position.y + size.y * 0.5, position.z),
        rotation: Quat::IDENTITY,
        size: Vec3::new(size.x, size.y, size.z),
    }))
}

pub(super) fn v3(v: axcenity_core::Vector3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}
