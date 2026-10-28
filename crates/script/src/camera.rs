//! `workspace.CurrentCamera`, which only LocalScripts reach: where the
//! player's view is, how it moves (`CameraType`), its lens
//! (`FieldOfView`), shakes, and turning screen points into rays and back.
//!
//! The host keeps it in step each frame: `set_camera` (its own camera, the
//! fly camera or a character controller's) and `set_viewport_size` before
//! scripts run, then `take_camera_move` and `step_camera` before drawing.
//! `step_camera` gives a [`GameCamera`], which turns the host's camera into
//! the one to draw. Everything here lives in the client interpreter's app
//! data, so the editor's Play and the player move it the same way.

use axcenity_core::{ClassName, InstanceId, Vector2, Vector3, World};
use axcenity_render::glam::camera::rh;
use axcenity_render::glam::{EulerRot, Mat4, Quat, Vec3};
use mlua::{FromLua, Lua, MetaMethod, UserData, UserDataFields, UserDataMethods};

use crate::cframe::{
    LuaCFrame, from_vec3, orientation_from_rotation, rotation_from_orientation, to_vec3,
};
use crate::instance::LuaInstance;
use crate::types::{EnumType, LuaEnumItem, LuaVector2, LuaVector3, f32_to_lua, type_name};

/// The vertical field of view a camera starts with, in degrees. The same
/// as the editor's and the player's fly camera (`scene::FlyCamera`).
pub const DEFAULT_FIELD_OF_VIEW: f32 = 50.0;
/// `FieldOfView` stays within these, in degrees.
const MIN_FIELD_OF_VIEW: f32 = 1.0;
const MAX_FIELD_OF_VIEW: f32 = 120.0;
/// Looking straight up or down has no "up" to draw with, so the drawn view
/// stops just short of it, as mouse look does.
const MAX_PITCH: f32 = 89.0;
/// How far `Focus` is in front of a camera that has nothing to focus on.
const FOCUS_DISTANCE: f32 = 10.0;
/// How far one unit of shake intensity moves the camera at its strongest:
/// studs, and degrees of turning.
const SHAKE_STUDS: f32 = 0.25;
const SHAKE_DEGREES: f32 = 1.0;

/// Where the camera is and which way it looks, in the same terms as a
/// Part: `orientation` is degrees around X, Y and Z (only X, looking up and
/// down, and Y, turning, change the view).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CameraState {
    pub position: Vector3,
    pub orientation: Vector3,
}

impl CameraState {
    fn cframe(self) -> LuaCFrame {
        LuaCFrame::new(
            to_vec3(self.position),
            rotation_from_orientation(self.orientation),
        )
    }

    fn from_cframe(cf: LuaCFrame) -> Self {
        Self {
            position: from_vec3(cf.position),
            orientation: orientation_from_rotation(cf.rotation),
        }
    }
}

/// `Enum.CameraType`: who moves the camera.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CameraType {
    /// The host's camera: a character controller's, or the fly camera.
    #[default]
    Custom,
    /// Only scripts move it, through `CFrame`.
    Scriptable,
    /// Follows `CameraSubject` from where it was, turning as the subject
    /// turns (around Y), so it stays behind it.
    Follow,
    /// Stays where it is.
    Fixed,
    /// Follows `CameraSubject` from where it was, without turning.
    Track,
}

impl CameraType {
    pub const ALL: [CameraType; 5] = [
        CameraType::Custom,
        CameraType::Scriptable,
        CameraType::Follow,
        CameraType::Fixed,
        CameraType::Track,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            CameraType::Custom => "Custom",
            CameraType::Scriptable => "Scriptable",
            CameraType::Follow => "Follow",
            CameraType::Fixed => "Fixed",
            CameraType::Track => "Track",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == name)
    }
}

/// A camera to draw: where it is, which way it looks and its vertical
/// field of view in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraView {
    pub position: Vector3,
    pub orientation: Vector3,
    pub field_of_view: f32,
}

impl CameraView {
    /// The view as the renderer draws it. Screen points and rays
    /// (`ScreenPointToRay`, `WorldToScreenPoint`) use the same camera.
    pub fn render_camera(self) -> axcenity_render::Camera {
        let pitch = self.orientation.x.clamp(-MAX_PITCH, MAX_PITCH);
        let look = look_vector(Vector3::new(pitch, self.orientation.y, 0.0));
        let position = to_vec3(self.position);
        axcenity_render::Camera {
            position,
            look_at: position + to_vec3(look),
            fov_y_degrees: self.field_of_view,
        }
    }
}

/// What the game's camera does this frame, from `ScriptRuntime::step_camera`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameCamera {
    /// Where scripts hold the camera: any `CameraType` but Custom. `None`
    /// leaves it to the host.
    pub scripted: Option<CameraState>,
    pub field_of_view: f32,
    /// The shake this frame: studs added to the position, and degrees to
    /// the orientation.
    pub shake: CameraState,
}

impl Default for GameCamera {
    fn default() -> Self {
        Self {
            scripted: None,
            field_of_view: DEFAULT_FIELD_OF_VIEW,
            shake: CameraState::default(),
        }
    }
}

impl GameCamera {
    /// The camera to draw, given the host's own (`host`): the scripted one
    /// instead when there is one, then shaken, with the script's lens.
    pub fn view(&self, host: CameraState) -> CameraView {
        let base = self.scripted.unwrap_or(host);
        let add = |a: Vector3, b: Vector3| from_vec3(to_vec3(a) + to_vec3(b));
        CameraView {
            position: add(base.position, self.shake.position),
            orientation: add(base.orientation, self.shake.orientation),
            field_of_view: self.field_of_view,
        }
    }
}

/// One `Camera:Shake`, fading out over its duration.
#[derive(Clone, Copy, Debug)]
struct Shake {
    intensity: f32,
    duration: f64,
    elapsed: f64,
}

impl Shake {
    /// How far it moves the camera now: studs and degrees. Waves at
    /// unrelated speeds make it look random, and the same on every computer.
    fn offset(&self) -> CameraState {
        let fade = (1.0 - self.elapsed / self.duration).clamp(0.0, 1.0);
        let strength = self.intensity * (fade * fade) as f32;
        let t = self.elapsed;
        let wave = |fast: f64, slow: f64, phase: f64| {
            ((t * fast + phase).sin() * 0.6 + (t * slow + phase * 1.7).sin() * 0.4) as f32
        };
        let studs = strength * SHAKE_STUDS;
        let degrees = strength * SHAKE_DEGREES;
        CameraState {
            position: Vector3::new(
                wave(23.0, 37.3, 0.0) * studs,
                wave(29.1, 41.7, 1.3) * studs,
                wave(19.7, 33.1, 2.9) * studs,
            ),
            orientation: Vector3::new(
                wave(31.3, 17.9, 4.1) * degrees,
                wave(27.7, 43.3, 5.3) * degrees,
                0.0,
            ),
        }
    }
}

/// The camera as scripts last saw or set it.
pub struct Camera {
    pub state: CameraState,
    /// The host's camera this frame, which a Custom camera is.
    host: CameraState,
    /// True once a script has moved the camera since the host last took
    /// the change. Only a Custom camera's moves are taken.
    pub moved: bool,
    pub camera_type: CameraType,
    pub field_of_view: f32,
    pub subject: Option<InstanceId>,
    /// `Focus`, once a script sets it.
    focus: Option<LuaCFrame>,
    /// The view's size in pixels, from the host.
    pub viewport: Vector2,
    /// Follow and Track: the subject, and the camera relative to it, from
    /// the first frame it followed that subject.
    hold: Option<(InstanceId, LuaCFrame)>,
    shakes: Vec<Shake>,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            state: CameraState::default(),
            host: CameraState::default(),
            moved: false,
            camera_type: CameraType::Custom,
            field_of_view: DEFAULT_FIELD_OF_VIEW,
            subject: None,
            focus: None,
            viewport: Vector2::new(1.0, 1.0),
            hold: None,
            shakes: Vec::new(),
        }
    }
}

impl Camera {
    /// The host's camera, before scripts run. A camera scripts hold (any
    /// type but Custom) stays where they put it.
    pub fn set_host(&mut self, state: CameraState) {
        self.host = state;
        if self.camera_type == CameraType::Custom {
            self.state = state;
        }
    }

    /// Where a script moved a Custom camera, if one did since the last call.
    pub fn take_move(&mut self) -> Option<CameraState> {
        let moved = std::mem::take(&mut self.moved);
        (moved && self.camera_type == CameraType::Custom).then_some(self.state)
    }

    /// Moves a following camera after its subject and the shakes on by
    /// `seconds`, and says what to draw.
    pub fn step(&mut self, world: &World, seconds: f64) -> GameCamera {
        let following = matches!(self.camera_type, CameraType::Follow | CameraType::Track);
        match self.subject.filter(|_| following) {
            Some(subject) => {
                if let Some(at) = subject_frame(world, subject) {
                    let anchor = if self.camera_type == CameraType::Track {
                        LuaCFrame::new(at.position, Quat::IDENTITY)
                    } else {
                        let (yaw, _, _) = at.rotation.to_euler(EulerRot::YXZ);
                        LuaCFrame::new(at.position, Quat::from_rotation_y(yaw))
                    };
                    let offset = match self.hold {
                        Some((held, offset)) if held == subject => offset,
                        _ => {
                            let offset = anchor.inverse() * self.state.cframe();
                            self.hold = Some((subject, offset));
                            offset
                        }
                    };
                    self.state = CameraState::from_cframe(anchor * offset);
                }
            }
            None => self.hold = None,
        }
        let mut shake = Vector3::default();
        let mut turn = Vector3::default();
        for active in &mut self.shakes {
            active.elapsed += seconds;
            let offset = active.offset();
            shake = from_vec3(to_vec3(shake) + to_vec3(offset.position));
            turn = from_vec3(to_vec3(turn) + to_vec3(offset.orientation));
        }
        self.shakes
            .retain(|active| active.elapsed < active.duration);
        GameCamera {
            scripted: (self.camera_type != CameraType::Custom).then_some(self.state),
            field_of_view: self.field_of_view,
            shake: CameraState {
                position: shake,
                orientation: turn,
            },
        }
    }

    fn view(&self) -> CameraView {
        CameraView {
            position: self.state.position,
            orientation: self.state.orientation,
            field_of_view: self.field_of_view,
        }
    }

    /// The ray from the camera through the pixel at `x`, `y` (from the
    /// view's top left), one stud long.
    pub fn ray(&self, x: f32, y: f32) -> Ray {
        let camera = self.view().render_camera();
        let (tan, aspect) = self.lens();
        let across = 2.0 * x / self.viewport.x.max(1.0) - 1.0;
        let up = 1.0 - 2.0 * y / self.viewport.y.max(1.0);
        let toward = Vec3::new(across * tan * aspect, up * tan, -1.0);
        let direction = view_matrix(&camera)
            .inverse()
            .transform_vector3(toward)
            .normalize();
        Ray {
            origin: from_vec3(camera.position),
            direction: from_vec3(direction),
        }
    }

    /// Where `point` shows on the view, in pixels from its top left, and
    /// how far in front of the camera it is (negative behind it).
    pub fn screen_point(&self, point: Vector3) -> Vector3 {
        let camera = self.view().render_camera();
        let (tan, aspect) = self.lens();
        let seen = view_matrix(&camera).transform_point3(to_vec3(point));
        let depth = -seen.z;
        // Points right at the camera have no place; keep them finite.
        let depth_or_tiny = if depth.abs() < 1e-6 { 1e-6 } else { depth };
        let across = seen.x / (depth_or_tiny * tan * aspect);
        let up = seen.y / (depth_or_tiny * tan);
        Vector3::new(
            (across + 1.0) / 2.0 * self.viewport.x,
            (1.0 - up) / 2.0 * self.viewport.y,
            depth,
        )
    }

    /// The tangent of half the vertical field of view, and the view's
    /// width over its height.
    fn lens(&self) -> (f32, f32) {
        let tan = (self.field_of_view.to_radians() / 2.0).tan();
        (tan, self.viewport.x.max(1.0) / self.viewport.y.max(1.0))
    }
}

/// The renderer's view matrix: the same `look_at` as `render` and
/// `scene::CameraView`, so rays match what is drawn.
fn view_matrix(camera: &axcenity_render::Camera) -> Mat4 {
    rh::view::look_at_mat4(camera.position, camera.look_at, Vec3::Y)
}

/// Where a `CameraSubject` is: a part's CFrame, or a Model's pivot turned
/// like its first part. A character controller or navigation agent stands
/// for the body it moves.
fn subject_frame(world: &World, id: InstanceId) -> Option<LuaCFrame> {
    let instance = world.get(id)?;
    let holder = if instance.class().is_character_controller()
        || instance.class() == ClassName::NavigationAgent
    {
        instance.parent()?
    } else {
        id
    };
    let class = world.get(holder)?.class();
    if !matches!(
        class,
        ClassName::Model
            | ClassName::Part
            | ClassName::MeshPart
            | ClassName::Seat
            | ClassName::VehicleSeat
    ) {
        return None;
    }
    let pivot = crate::cframe::pivot(world, holder).ok()?;
    if class != ClassName::Model {
        return Some(pivot);
    }
    let turned = axcenity_character::body_parts(world, holder)
        .first()
        .and_then(|&part| world.get(part))
        .and_then(LuaCFrame::of_instance)
        .map_or(Quat::IDENTITY, |part| part.rotation);
    Some(LuaCFrame::new(pivot.position, turned))
}

/// The direction a camera with this orientation looks in, one stud long.
/// Turning around Y by `yaw` and then tilting up by `pitch` takes the
/// starting direction (straight ahead, -Z) here; rolling around Z doesn't
/// change where it points.
pub fn look_vector(orientation: Vector3) -> Vector3 {
    let pitch = orientation.x.to_radians();
    let yaw = orientation.y.to_radians();
    Vector3::new(
        -yaw.sin() * pitch.cos(),
        pitch.sin(),
        -yaw.cos() * pitch.cos(),
    )
}

/// A half-line: `ScreenPointToRay`'s result. `Direction` is one stud long.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub origin: Vector3,
    pub direction: Vector3,
}

impl UserData for Ray {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Origin", |_, ray| Ok(LuaVector3(ray.origin)));
        fields.add_field_method_get("Direction", |_, ray| Ok(LuaVector3(ray.direction)));
        fields.add_field_method_get("Unit", |_, ray| Ok(*ray));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, ray, ()| {
            let (o, d) = (ray.origin, ray.direction);
            Ok(format!(
                "{{{}, {}, {}}}, {{{}, {}, {}}}",
                o.x, o.y, o.z, d.x, d.y, d.z
            ))
        });
    }
}

/// `workspace.CurrentCamera`. Holds nothing itself: the state is the
/// `Camera` in app data, so every read sees the latest values.
#[derive(Clone, Copy)]
pub struct LuaCamera;

fn camera_ref(lua: &Lua) -> mlua::Result<mlua::AppDataRef<'_, Camera>> {
    lua.app_data_ref::<Camera>()
        .ok_or_else(|| mlua::Error::runtime("the camera isn't available here"))
}

fn camera_mut(lua: &Lua) -> mlua::Result<mlua::AppDataRefMut<'_, Camera>> {
    lua.app_data_mut::<Camera>()
        .ok_or_else(|| mlua::Error::runtime("the camera isn't available here"))
}

/// Moves the camera, as a script setting `CFrame`, `Position` or
/// `Orientation` does (TweenService too).
pub(crate) fn set_state(lua: &Lua, state: CameraState) -> mlua::Result<()> {
    let mut camera = camera_mut(lua)?;
    camera.state = state;
    camera.moved = true;
    // A script moving a following camera moves where it follows from.
    camera.hold = None;
    Ok(())
}

pub(crate) fn state(lua: &Lua) -> mlua::Result<CameraState> {
    Ok(camera_ref(lua)?.state)
}

pub(crate) fn cframe(lua: &Lua) -> mlua::Result<LuaCFrame> {
    Ok(state(lua)?.cframe())
}

pub(crate) fn set_cframe(lua: &Lua, cf: LuaCFrame) -> mlua::Result<()> {
    set_state(lua, CameraState::from_cframe(cf))
}

pub(crate) fn field_of_view(lua: &Lua) -> mlua::Result<f32> {
    Ok(camera_ref(lua)?.field_of_view)
}

pub(crate) fn set_field_of_view(lua: &Lua, degrees: f64) -> mlua::Result<()> {
    if !degrees.is_finite() {
        return Err(mlua::Error::runtime("Camera.FieldOfView must be a number"));
    }
    camera_mut(lua)?.field_of_view = (degrees as f32).clamp(MIN_FIELD_OF_VIEW, MAX_FIELD_OF_VIEW);
    Ok(())
}

/// Sets one of the camera's Vector3 fields, and marks the camera moved.
fn set_vector(
    lua: &Lua,
    field: &str,
    value: mlua::Value,
    apply: impl FnOnce(&mut CameraState, Vector3),
) -> mlua::Result<()> {
    let vector = LuaVector3::from_lua(value.clone(), lua).map_err(|_| {
        mlua::Error::runtime(format!(
            "Camera.{field} must be a Vector3, not a {}",
            type_name(lua, &value)
        ))
    })?;
    let mut state = state(lua)?;
    apply(&mut state, vector.0);
    set_state(lua, state)
}

fn number_arg(lua: &Lua, value: &mlua::Value, what: &str) -> mlua::Result<f32> {
    match value {
        mlua::Value::Integer(n) => Ok(*n as f32),
        mlua::Value::Number(n) if n.is_finite() => Ok(*n as f32),
        _ => Err(mlua::Error::runtime(format!(
            "{what} must be a number, not a {}",
            type_name(lua, value)
        ))),
    }
}

/// `ScreenPointToRay(x, y, depth)`: the ray through a pixel, starting
/// `depth` studs in front of the camera.
fn screen_ray(
    lua: &Lua,
    (x, y, depth): (mlua::Value, mlua::Value, Option<f64>),
) -> mlua::Result<Ray> {
    let x = number_arg(lua, &x, "ScreenPointToRay's x")?;
    let y = number_arg(lua, &y, "ScreenPointToRay's y")?;
    let mut ray = camera_ref(lua)?.ray(x, y);
    let depth = depth.unwrap_or(0.0) as f32;
    ray.origin = from_vec3(to_vec3(ray.origin) + to_vec3(ray.direction) * depth);
    Ok(ray)
}

/// `WorldToScreenPoint(point)`: the pixel and depth, and whether it's on
/// the view (in front of the camera and inside the view's edges).
fn screen_point(lua: &Lua, point: mlua::Value) -> mlua::Result<(LuaVector3, bool)> {
    let point = LuaVector3::from_lua(point.clone(), lua).map_err(|_| {
        mlua::Error::runtime(format!(
            "WorldToScreenPoint takes a Vector3, not a {}",
            type_name(lua, &point)
        ))
    })?;
    let camera = camera_ref(lua)?;
    let at = camera.screen_point(point.0);
    let size = camera.viewport;
    let on_screen = at.z > 0.0 && (0.0..=size.x).contains(&at.x) && (0.0..=size.y).contains(&at.y);
    Ok((LuaVector3(at), on_screen))
}

impl UserData for LuaCamera {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("CFrame", |lua, _| cframe(lua));
        fields.add_field_method_set("CFrame", |lua, _, value: mlua::Value| {
            let cf = LuaCFrame::from_lua(value.clone(), lua).map_err(|_| {
                mlua::Error::runtime(format!(
                    "Camera.CFrame must be a CFrame, not a {}",
                    type_name(lua, &value)
                ))
            })?;
            set_cframe(lua, cf)
        });
        fields.add_field_method_get("Position", |lua, _| Ok(LuaVector3(state(lua)?.position)));
        fields.add_field_method_set("Position", |lua, _, value: mlua::Value| {
            set_vector(lua, "Position", value, |state, v| state.position = v)
        });
        fields.add_field_method_get("Orientation", |lua, _| {
            Ok(LuaVector3(state(lua)?.orientation))
        });
        fields.add_field_method_set("Orientation", |lua, _, value: mlua::Value| {
            set_vector(lua, "Orientation", value, |state, v| state.orientation = v)
        });
        fields.add_field_method_get("LookVector", |lua, _| {
            Ok(LuaVector3(look_vector(state(lua)?.orientation)))
        });
        fields.add_field_method_get("FieldOfView", |lua, _| Ok(f32_to_lua(field_of_view(lua)?)));
        fields.add_field_method_set("FieldOfView", |lua, _, value: mlua::Value| {
            let degrees = number_arg(lua, &value, "Camera.FieldOfView")?;
            set_field_of_view(lua, f64::from(degrees))
        });
        fields.add_field_method_get("CameraType", |lua, _| {
            let kind = camera_ref(lua)?.camera_type;
            Ok(LuaEnumItem::find(EnumType::CameraType, kind.as_str()))
        });
        fields.add_field_method_set("CameraType", |lua, _, value: mlua::Value| {
            let item = LuaEnumItem::from_lua(value.clone(), lua)
                .ok()
                .filter(|item| item.enum_type == EnumType::CameraType)
                .ok_or_else(|| {
                    mlua::Error::runtime(format!(
                        "Camera.CameraType must be an Enum.CameraType, such as \
                         Enum.CameraType.Scriptable, not a {}",
                        type_name(lua, &value)
                    ))
                })?;
            let kind = CameraType::from_name(item.name).unwrap_or_default();
            let mut camera = camera_mut(lua)?;
            if camera.camera_type != kind {
                camera.camera_type = kind;
                camera.hold = None;
                camera.moved = false;
                // Back to Custom, scripts see the host's camera at once.
                if kind == CameraType::Custom {
                    camera.state = camera.host;
                }
            }
            Ok(())
        });
        fields.add_field_method_get("CameraSubject", |lua, _| {
            Ok(camera_ref(lua)?.subject.map(LuaInstance))
        });
        fields.add_field_method_set("CameraSubject", |lua, _, value: mlua::Value| {
            let subject = match &value {
                mlua::Value::Nil => None,
                _ => Some(LuaInstance::from_lua(value.clone(), lua).map_err(|_| {
                    mlua::Error::runtime(format!(
                        "Camera.CameraSubject must be a Model, a part, a character controller \
                         or nil, not a {}",
                        type_name(lua, &value)
                    ))
                })?),
            };
            let mut camera = camera_mut(lua)?;
            camera.subject = subject.map(|subject| subject.0);
            camera.hold = None;
            Ok(())
        });
        fields.add_field_method_get("Focus", |lua, _| {
            let (focus, subject, state) = {
                let camera = camera_ref(lua)?;
                (camera.focus, camera.subject, camera.state)
            };
            if let Some(focus) = focus {
                return Ok(focus);
            }
            let world = crate::instance::world(lua)?;
            if let Some(at) = subject.and_then(|subject| subject_frame(&world, subject)) {
                return Ok(LuaCFrame::new(at.position, Quat::IDENTITY));
            }
            let ahead =
                to_vec3(state.position) + to_vec3(look_vector(state.orientation)) * FOCUS_DISTANCE;
            Ok(LuaCFrame::new(ahead, Quat::IDENTITY))
        });
        fields.add_field_method_set("Focus", |lua, _, value: mlua::Value| {
            let cf = LuaCFrame::from_lua(value.clone(), lua).map_err(|_| {
                mlua::Error::runtime(format!(
                    "Camera.Focus must be a CFrame, not a {}",
                    type_name(lua, &value)
                ))
            })?;
            camera_mut(lua)?.focus = Some(cf);
            Ok(())
        });
        fields.add_field_method_get("ViewportSize", |lua, _| {
            Ok(LuaVector2(camera_ref(lua)?.viewport))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("ScreenPointToRay", |lua, _, args| screen_ray(lua, args));
        // There's no top bar taking part of the screen, so the viewport is
        // the screen.
        methods.add_method("ViewportPointToRay", |lua, _, args| screen_ray(lua, args));
        methods.add_method("WorldToScreenPoint", |lua, _, point| {
            screen_point(lua, point)
        });
        methods.add_method("WorldToViewportPoint", |lua, _, point| {
            screen_point(lua, point)
        });
        methods.add_method(
            "Shake",
            |lua, _, (intensity, duration): (mlua::Value, mlua::Value)| {
                let intensity = number_arg(lua, &intensity, "Shake's intensity")?;
                let duration = number_arg(lua, &duration, "Shake's duration")?;
                if intensity > 0.0 && duration > 0.0 {
                    camera_mut(lua)?.shakes.push(Shake {
                        intensity,
                        duration: f64::from(duration),
                        elapsed: 0.0,
                    });
                }
                Ok(())
            },
        );
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("Camera"));
    }
}

/// How far `Mouse.Target` and `Mouse.Hit` look, in studs.
const MOUSE_REACH: f32 = 1000.0;

/// `Players.LocalPlayer:GetMouse()`: the mouse in the world, through the
/// camera. `Target` is the part under it, so "part under mouse" is
/// `ScreenPointToRay` and `Raycast` in one.
#[derive(Clone, Copy)]
pub struct LuaMouse;

impl LuaMouse {
    fn ray(lua: &Lua) -> mlua::Result<Ray> {
        let [x, y] = crate::input::mouse_position(lua)?;
        Ok(camera_ref(lua)?.ray(x, y))
    }

    /// The first part the mouse's ray hits, and where.
    fn hit(lua: &Lua) -> mlua::Result<(Ray, Option<axcenity_physics::RayHit>)> {
        let ray = Self::ray(lua)?;
        let assets = lua
            .app_data_ref::<axcenity_assets::Assets>()
            .as_deref()
            .cloned()
            .unwrap_or_default();
        let direction = from_vec3(to_vec3(ray.direction) * MOUSE_REACH);
        let hit = {
            let world = crate::instance::world(lua)?;
            axcenity_physics::cast_ray(&world, &assets, ray.origin, direction)
        };
        Ok((ray, hit))
    }
}

impl UserData for LuaMouse {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("X", |lua, _| {
            Ok(f32_to_lua(crate::input::mouse_position(lua)?[0]))
        });
        fields.add_field_method_get("Y", |lua, _| {
            Ok(f32_to_lua(crate::input::mouse_position(lua)?[1]))
        });
        fields.add_field_method_get("UnitRay", |lua, _| Self::ray(lua));
        fields.add_field_method_get("Target", |lua, _| {
            Ok(Self::hit(lua)?.1.map(|hit| LuaInstance(hit.instance)))
        });
        fields.add_field_method_get("Hit", |lua, _| {
            let (ray, hit) = Self::hit(lua)?;
            let at = match hit {
                Some(hit) => to_vec3(hit.position),
                None => to_vec3(ray.origin) + to_vec3(ray.direction) * MOUSE_REACH,
            };
            Ok(LuaCFrame::new(at, Quat::IDENTITY))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("Mouse"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vector3, b: Vector3) -> bool {
        (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4 && (a.z - b.z).abs() < 1e-4
    }

    #[test]
    fn look_vector_turns_with_yaw_and_tilts_with_pitch() {
        let ahead = look_vector(Vector3::new(0.0, 0.0, 0.0));
        assert!(close(ahead, Vector3::new(0.0, 0.0, -1.0)), "{ahead:?}");
        // Turning left (positive Y) by 90 degrees looks along -X.
        let left = look_vector(Vector3::new(0.0, 90.0, 0.0));
        assert!(close(left, Vector3::new(-1.0, 0.0, 0.0)), "{left:?}");
        let up = look_vector(Vector3::new(90.0, 0.0, 0.0));
        assert!(close(up, Vector3::new(0.0, 1.0, 0.0)), "{up:?}");
        // Roll doesn't change where it looks.
        let rolled = look_vector(Vector3::new(0.0, 0.0, 45.0));
        assert!(close(rolled, ahead), "{rolled:?}");
    }

    #[test]
    fn a_screen_point_and_its_ray_lead_back_to_each_other_at_any_view_size() {
        for (width, height) in [(800.0, 600.0), (1920.0, 1080.0), (300.0, 900.0)] {
            let camera = Camera {
                state: CameraState {
                    position: Vector3::new(3.0, 8.0, 20.0),
                    orientation: Vector3::new(-20.0, 35.0, 0.0),
                },
                viewport: Vector2::new(width, height),
                field_of_view: 70.0,
                ..Camera::default()
            };
            let middle = camera.ray(width / 2.0, height / 2.0);
            assert!(close(
                middle.direction,
                look_vector(camera.state.orientation)
            ));
            let ray = camera.ray(width * 0.2, height * 0.85);
            let point = from_vec3(to_vec3(ray.origin) + to_vec3(ray.direction) * 30.0);
            let back = camera.screen_point(point);
            assert!(
                (back.x - width * 0.2).abs() < 1e-2 && (back.y - height * 0.85).abs() < 1e-2,
                "{back:?} at {width}x{height}"
            );
            let look = to_vec3(look_vector(camera.state.orientation));
            assert!((back.z - 30.0 * to_vec3(ray.direction).dot(look)).abs() < 1e-3);
        }
    }

    #[test]
    fn a_shake_fades_out_and_ends_on_its_own() {
        let mut camera = Camera::default();
        camera.shakes.push(Shake {
            intensity: 2.0,
            duration: 0.5,
            elapsed: 0.0,
        });
        let world = World::new();
        let first = camera.step(&world, 0.05);
        assert_ne!(first.shake.position, Vector3::default());
        assert!(
            first.scripted.is_none(),
            "Custom leaves the camera to the host"
        );
        let mut last = first;
        for _ in 0..20 {
            last = camera.step(&world, 0.05);
        }
        assert_eq!(last.shake, CameraState::default());
        assert!(camera.shakes.is_empty());
    }
}
