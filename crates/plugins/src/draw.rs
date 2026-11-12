//! Viewport drawing (roadmap G10): `plugin:DrawLine`, `DrawBox`,
//! `DrawSphere`, `DrawText` and `ClearDrawings`. Drawings are kept until
//! the plugin removes them (retained, not redrawn every frame), so a plugin
//! draws once when something changes. The editor draws them over the
//! Scene View while editing ([`crate::Plugin::take_drawings`]).

use std::collections::BTreeMap;

use axcenity_core::{CFrame, Color3, Vector3};
use axcenity_script::system::{LuaCFrame, LuaColor3, LuaVector3};
use mlua::{FromLua, Lua, MetaMethod, Table, UserData, UserDataMethods, Value};

use crate::api::state;

/// How many drawings one plugin may keep at once.
pub const MAX_DRAWINGS: usize = 10_000;
/// The longest text a `DrawText` shows, in characters.
const MAX_TEXT: usize = 200;
/// A line's thickness unless the options give one, in points.
pub const DEFAULT_THICKNESS: f32 = 2.0;
/// The colour unless the options give one: the editor's plugin orange.
pub const DEFAULT_COLOR: Color3 = Color3 {
    r: 1.0,
    g: 0.6,
    b: 0.15,
};

/// What a drawing shows, in world space.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Line {
        from: Vector3,
        to: Vector3,
    },
    /// The edges of a box `size` across, centred on `cframe`.
    Box {
        cframe: CFrame,
        size: Vector3,
    },
    /// Three rings round `center`, one in each axis plane.
    Sphere {
        center: Vector3,
        radius: f32,
    },
    /// A label at `at`, the same size however far away.
    Text {
        at: Vector3,
        text: String,
    },
}

/// One thing a plugin drew.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawing {
    pub shape: Shape,
    pub color: Color3,
    /// The line width in points, or the text size for `Text`.
    pub thickness: f32,
}

/// A plugin's drawings, by number, and whether they changed since the
/// host last took them.
#[derive(Default)]
pub(crate) struct Drawings {
    pub(crate) shapes: BTreeMap<u64, Drawing>,
    pub(crate) next: u64,
    pub(crate) changed: bool,
}

impl Drawings {
    fn add(&mut self, drawing: Drawing) -> mlua::Result<LuaDrawing> {
        if self.shapes.len() >= MAX_DRAWINGS {
            return Err(mlua::Error::runtime(format!(
                "a plugin can keep at most {MAX_DRAWINGS} drawings: remove some, or call plugin:ClearDrawings()"
            )));
        }
        self.next += 1;
        self.shapes.insert(self.next, drawing);
        self.changed = true;
        Ok(LuaDrawing(self.next))
    }
}

/// The options table's `Color` and `Thickness`, with `thickness` as the
/// default width.
fn options(lua: &Lua, options: Option<Table>, thickness: f32) -> mlua::Result<(Color3, f32)> {
    let Some(options) = options else {
        return Ok((DEFAULT_COLOR, thickness));
    };
    let color = match options.get::<Value>("Color")? {
        Value::Nil => DEFAULT_COLOR,
        value => LuaColor3::from_lua(value, lua)?.0,
    };
    let thickness = match options.get::<Option<f32>>("Thickness")? {
        Some(t) if t.is_finite() => t.clamp(0.5, 32.0),
        _ => thickness,
    };
    Ok((color, thickness))
}

fn finite(v: Vector3, what: &str) -> mlua::Result<Vector3> {
    if v.x.is_finite() && v.y.is_finite() && v.z.is_finite() {
        Ok(v)
    } else {
        Err(mlua::Error::runtime(format!(
            "{what} must be a finite Vector3"
        )))
    }
}

fn add(lua: &Lua, shape: Shape, style: (Color3, f32)) -> mlua::Result<LuaDrawing> {
    state(lua)?.drawings.add(Drawing {
        shape,
        color: style.0,
        thickness: style.1,
    })
}

/// `plugin:DrawLine(from, to, options)`.
pub(crate) fn line(
    lua: &Lua,
    (from, to, opts): (LuaVector3, LuaVector3, Option<Table>),
) -> mlua::Result<LuaDrawing> {
    let shape = Shape::Line {
        from: finite(from.0, "from")?,
        to: finite(to.0, "to")?,
    };
    add(lua, shape, options(lua, opts, DEFAULT_THICKNESS)?)
}

/// `plugin:DrawBox(where, size, options)`: `where` is a CFrame, or a
/// Vector3 for a box lined up with the world.
pub(crate) fn boxed(
    lua: &Lua,
    (at, size, opts): (Value, LuaVector3, Option<Table>),
) -> mlua::Result<LuaDrawing> {
    let cframe = match &at {
        Value::UserData(data) if data.is::<LuaCFrame>() => LuaCFrame::from_lua(at, lua)?.to_core(),
        _ => CFrame {
            position: finite(LuaVector3::from_lua(at, lua)?.0, "the box's position")?,
            ..CFrame::IDENTITY
        },
    };
    finite(cframe.position, "the box's position")?;
    let size = finite(size.0, "size")?;
    let shape = Shape::Box {
        cframe,
        size: Vector3::new(size.x.abs(), size.y.abs(), size.z.abs()),
    };
    add(lua, shape, options(lua, opts, DEFAULT_THICKNESS)?)
}

/// `plugin:DrawSphere(center, radius, options)`.
pub(crate) fn sphere(
    lua: &Lua,
    (center, radius, opts): (LuaVector3, f32, Option<Table>),
) -> mlua::Result<LuaDrawing> {
    if !radius.is_finite() {
        return Err(mlua::Error::runtime("radius must be a finite number"));
    }
    let shape = Shape::Sphere {
        center: finite(center.0, "center")?,
        radius: radius.abs(),
    };
    add(lua, shape, options(lua, opts, DEFAULT_THICKNESS)?)
}

/// `plugin:DrawText(position, text, options)`: `Thickness` is the text
/// size here (14 points unless given).
pub(crate) fn text(
    lua: &Lua,
    (at, text, opts): (LuaVector3, String, Option<Table>),
) -> mlua::Result<LuaDrawing> {
    let text: String = text.chars().take(MAX_TEXT).collect();
    let shape = Shape::Text {
        at: finite(at.0, "position")?,
        text,
    };
    add(lua, shape, options(lua, opts, 14.0)?)
}

/// `plugin:ClearDrawings()`.
pub(crate) fn clear(lua: &Lua, (): ()) -> mlua::Result<()> {
    let mut state = state(lua)?;
    if !state.drawings.shapes.is_empty() {
        state.drawings.shapes.clear();
        state.drawings.changed = true;
    }
    Ok(())
}

/// A drawing, by its number, so the plugin can recolour or remove it.
pub(crate) struct LuaDrawing(u64);

impl UserData for LuaDrawing {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("Remove", |lua, this, ()| {
            let mut state = state(lua)?;
            if state.drawings.shapes.remove(&this.0).is_some() {
                state.drawings.changed = true;
            }
            Ok(())
        });
        methods.add_method("SetColor", |lua, this, color: LuaColor3| {
            let mut state = state(lua)?;
            if let Some(drawing) = state.drawings.shapes.get_mut(&this.0)
                && drawing.color != color.0
            {
                drawing.color = color.0;
                state.drawings.changed = true;
            }
            Ok(())
        });
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("PluginDrawing"));
    }
}
