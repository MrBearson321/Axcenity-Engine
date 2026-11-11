//! Surface shaders: a material's own WGSL `surface` function, which works
//! out each pixel's surface in place of the standard material's.
//!
//! The scene shader is a template (`SCENE_TEMPLATE`: the sky, the lighting
//! and shader.wgsl) followed by one `surface` function: surface.wgsl's for
//! the standard material, or a material's own code. So a line of the
//! material's code is always the same number of lines into the module, and
//! an error is reported on the material's own line. Each shader is checked
//! with naga before the GPU sees it, compiled into its own pipelines once
//! (cached by the code's hash, like meshes by their key) and dropped after
//! [`KEEP_UNDRAWN_FRAMES`] frames without a draw. One that doesn't compile
//! is reported once and its items draw with surface_error.wgsl's glowing
//! magenta checks.

use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use wgpu::naga;

use super::gpu::{SCENE_TEMPLATE, create_pipeline};
use super::{Cached, HDR_FORMAT, ItemPipelines, KEEP_UNDRAWN_FRAMES, NORMAL_FORMAT, ScenePipeline};

/// A material's WGSL `surface` function (see [`crate::Material::surface_shader`]).
/// Cheap to clone: the code is shared.
#[derive(Clone)]
pub struct SurfaceShader {
    material: Arc<str>,
    code: Arc<str>,
    key: u64,
}

impl SurfaceShader {
    /// The shader `code` of the material named `material` (its file's
    /// path, used in error messages).
    pub fn new(material: &str, code: &str) -> Self {
        let mut hasher = std::hash::DefaultHasher::new();
        code.hash(&mut hasher);
        Self {
            material: material.into(),
            code: code.into(),
            key: hasher.finish(),
        }
    }

    pub fn material(&self) -> &str {
        &self.material
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    /// The same for the same code, so materials with the same code share
    /// their pipelines.
    pub fn key(&self) -> u64 {
        self.key
    }

    /// Whether the code compiles in the scene shader, or its first error,
    /// on the code's own line. Doesn't need a GPU, so the editor checks
    /// code as it's written.
    pub fn check(&self) -> Result<(), ShaderError> {
        let source = module_source(&self.code);
        let located = |location: Option<naga::SourceLocation>, message: String| {
            // Lines before the code's are the engine's: an error found
            // there is the code's fault (it's called from there).
            let line = location.and_then(|at| at.line_number.checked_sub(template_lines()));
            let message = match line {
                Some(0) | None => format!(
                    "{message} (where the engine calls it: the code must define `fn \
                     surface(input: SurfaceInput) -> Surface`)"
                ),
                Some(_) => message,
            };
            self.error(
                line.filter(|&line| line > 0),
                location.map(|at| at.line_position),
                message,
            )
        };
        let module = naga::front::wgsl::parse_str(&source)
            .map_err(|err| located(err.location(&source), parse_message(&err)))?;
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            validator_capabilities(),
        )
        .validate(&module)
        .map_err(|err| {
            // The last place named is the most particular: the
            // expression rather than the function around it.
            let at = err.spans().last().map(|(span, _)| span.location(&source));
            located(at, validation_message(&err, &source))
        })?;
        Ok(())
    }

    fn error(&self, line: Option<u32>, column: Option<u32>, message: String) -> ShaderError {
        ShaderError {
            material: self.material.to_string(),
            line,
            column: column.filter(|_| line.is_some()),
            message,
        }
    }
}

impl fmt::Debug for SurfaceShader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SurfaceShader({} #{:x})", self.material, self.key)
    }
}

/// Why a material's surface shader can't be drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShaderError {
    /// The material's name ([`SurfaceShader::material`]).
    pub material: String,
    /// The line of the material's code, from 1, or `None` when the error
    /// isn't on one of them.
    pub line: Option<u32>,
    /// The byte on that line where it starts, from 1.
    pub column: Option<u32>,
    pub message: String,
}

impl fmt::Display for ShaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let material = &self.material;
        match self.line {
            Some(line) => write!(
                f,
                "The shader of the material `{material}` has an error on line {line}: {}",
                self.message
            ),
            None => write!(
                f,
                "The shader of the material `{material}` has an error: {}",
                self.message
            ),
        }
    }
}

/// The scene shader with `code` as its `surface` function.
fn module_source(code: &str) -> String {
    let mut source = String::with_capacity(SCENE_TEMPLATE.len() + code.len() + 1);
    source.push_str(SCENE_TEMPLATE);
    if !source.ends_with('\n') {
        source.push('\n');
    }
    source.push_str(code);
    source
}

/// How many lines come before a material's code in [`module_source`].
fn template_lines() -> u32 {
    let lines = SCENE_TEMPLATE.matches('\n').count() + usize::from(!SCENE_TEMPLATE.ends_with('\n'));
    lines as u32
}

/// What the scene shader may use: what every backend the engine runs on
/// has (`wgpu` checks again against the device's own).
fn validator_capabilities() -> naga::valid::Capabilities {
    naga::valid::Capabilities::CUBE_ARRAY_TEXTURES
}

fn parse_message(err: &naga::front::wgsl::ParseError) -> String {
    let message = err.message().to_owned();
    match err.labels().next() {
        Some((_, label)) if !label.is_empty() && !message.contains(label) => {
            format!("{message} ({label})")
        }
        _ => message,
    }
}

/// A validation error and its causes on one line, with the code each
/// expression it names (`[5]`) stands for.
fn validation_message(err: &naga::WithSpan<naga::valid::ValidationError>, source: &str) -> String {
    let mut message = with_causes(err.as_inner());
    for (span, label) in err.spans() {
        let Some(range) = span.to_range() else {
            continue;
        };
        if let Some(handle) = label.strip_prefix("naga::ir::Expression ")
            && let Some(code) = source.get(range)
        {
            let code: String = code.split_whitespace().collect::<Vec<_>>().join(" ");
            message = message.replace(handle, &format!("`{code}`"));
        } else if let Some(handle) = label.strip_prefix("naga::ir::Function ") {
            message = message.replace(&format!("Function {handle} "), "The function ");
        }
    }
    message
}

/// An error and its causes on one line.
fn with_causes(err: &dyn std::error::Error) -> String {
    let mut text = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        text.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    text
}

/// The pipelines that draw items with one surface shader: the scene
/// pass's four, and the depth and normal prepass's two (one-sided, then
/// double-sided).
pub(super) struct SurfacePipelines {
    pub(super) items: ItemPipelines,
    pub(super) prepass: [wgpu::RenderPipeline; 2],
}

impl SurfacePipelines {
    /// The prepass pipeline for items drawn `double_sided` or not.
    pub(super) fn prepass(&self, double_sided: bool) -> &wgpu::RenderPipeline {
        &self.prepass[usize::from(double_sided)]
    }
}

/// Every surface shader drawn lately, compiled for the scene's samples
/// per pixel.
#[derive(Default)]
pub(super) struct SurfaceShaders {
    /// By [`SurfaceShader::key`]: its pipelines, or `None` when it doesn't
    /// compile (so it's reported once).
    compiled: HashMap<u64, Cached<SurfacePipelines>>,
    /// surface_error.wgsl's, while a shader that doesn't compile is drawn.
    broken: Option<SurfacePipelines>,
    /// Errors not taken yet.
    errors: Vec<ShaderError>,
}

impl SurfaceShaders {
    /// The pipelines to draw `key`'s items with: its own, the error
    /// surface's if it doesn't compile, or `None` if it wasn't prepared.
    pub(super) fn get(&self, key: u64) -> Option<&SurfacePipelines> {
        match &self.compiled.get(&key)?.gpu {
            Some(pipelines) => Some(pipelines),
            None => self.broken.as_ref(),
        }
    }

    /// How many surface shaders have pipelines, and how many don't compile.
    pub(super) fn counts(&self) -> (u32, u32) {
        let compiled = self.compiled.values().filter(|cached| cached.gpu.is_some());
        let compiled = compiled.count() as u32;
        (compiled, self.compiled.len() as u32 - compiled)
    }

    /// How many surface shaders have pipelines, and whether the error
    /// surface's are alive.
    #[cfg(test)]
    pub(super) fn alive(&self) -> (usize, bool) {
        let compiled = self.compiled.values().filter(|cached| cached.gpu.is_some());
        (compiled.count(), self.broken.is_some())
    }

    /// Frees every pipeline, to be compiled again for new samples per
    /// pixel. Shaders that don't compile stay known, so they aren't
    /// reported again.
    pub(super) fn clear_pipelines(&mut self) {
        self.compiled.retain(|_, cached| cached.gpu.is_none());
        self.broken = None;
    }

    /// Frees the shaders not drawn for [`KEEP_UNDRAWN_FRAMES`] frames.
    pub(super) fn forget(&mut self, frame: u64) {
        self.compiled
            .retain(|_, cached| frame - cached.last_drawn < KEEP_UNDRAWN_FRAMES);
        if self.compiled.values().all(|cached| cached.gpu.is_some()) {
            self.broken = None;
        }
    }

    pub(super) fn take_errors(&mut self) -> Vec<ShaderError> {
        std::mem::take(&mut self.errors)
    }
}

impl ScenePipeline {
    /// Compiles `shader` if it's new, and marks it drawn this frame.
    pub(super) fn prepare_surface_shader(&mut self, shader: &SurfaceShader) {
        let samples = self.graphics.anti_aliasing.samples();
        let layouts = [
            &self.frame_layout,
            &self.material_layout,
            &self.skinning.layout,
        ];
        let shaders = &mut self.surface_shaders;
        let compiled = match shaders.compiled.entry(shader.key) {
            Entry::Occupied(mut cached) => {
                cached.get_mut().last_drawn = self.frame;
                cached.get().gpu.is_some()
            }
            Entry::Vacant(slot) => {
                let pipelines = compile(&self.device, shader, layouts, samples);
                let compiled = pipelines.is_ok();
                if let Err(error) = &pipelines {
                    log::warn!("{error}");
                    shaders.errors.push(error.clone());
                }
                slot.insert(Cached {
                    gpu: pipelines.ok(),
                    last_drawn: self.frame,
                });
                compiled
            }
        };
        if !compiled && shaders.broken.is_none() {
            let source = module_source(include_str!("../surface_error.wgsl"));
            let module = self
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("broken surface"),
                    source: wgpu::ShaderSource::Wgsl(Cow::Owned(source)),
                });
            shaders.broken = Some(pipelines_of(&self.device, &module, layouts, samples));
        }
    }
}

/// `shader`'s pipelines, or why it can't have them: naga's check first,
/// then anything the GPU's own compiler finds.
fn compile(
    device: &wgpu::Device,
    shader: &SurfaceShader,
    layouts: [&wgpu::BindGroupLayout; 3],
    samples: u32,
) -> Result<SurfacePipelines, ShaderError> {
    shader.check()?;
    // Errors in these scopes are handed back here rather than to the
    // device's handler (which would end the program).
    let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(shader.material()),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(module_source(shader.code()))),
    });
    let pipelines = pipelines_of(device, &module, layouts, samples);
    // Scopes pop in the opposite order to the one they were pushed in.
    let validation = ready(validation.pop()).flatten();
    let internal = ready(internal.pop()).flatten();
    match validation.or(internal) {
        Some(err) => Err(shader.error(None, None, with_causes(&err))),
        None => Ok(pipelines),
    }
}

fn pipelines_of(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layouts: [&wgpu::BindGroupLayout; 3],
    samples: u32,
) -> SurfacePipelines {
    let item = |transparent, double_sided| {
        create_pipeline(
            device,
            module,
            ("vs_main", "fs_main"),
            &layouts,
            (HDR_FORMAT, samples),
            (transparent, double_sided),
        )
    };
    let prepass = |double_sided| {
        create_pipeline(
            device,
            module,
            ("vs_main", "fs_prepass"),
            &layouts,
            (NORMAL_FORMAT, 1),
            (false, double_sided),
        )
    };
    SurfacePipelines {
        items: ItemPipelines {
            opaque: item(false, false),
            transparent: item(true, false),
            double_sided: item(false, true),
            double_sided_transparent: item(true, true),
        },
        prepass: [prepass(false), prepass(true)],
    }
}

/// What `future` gives, if it's ready at once. wgpu's error scopes are,
/// on every native backend.
fn ready<T>(future: impl Future<Output = T>) -> Option<T> {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_standard_surface_checks_and_an_error_is_on_the_codes_own_line() {
        let standard = SurfaceShader::new("standard", include_str!("../surface.wgsl"));
        assert_eq!(standard.check(), Ok(()));
        let broken = SurfaceShader::new("broken", include_str!("../surface_error.wgsl"));
        assert_eq!(broken.check(), Ok(()));

        let code = "fn surface(input: SurfaceInput) -> Surface {\n    var out = \
                    standard_surface(input);\n    out.roughness = 0.2\n    return out;\n}\n";
        let error = SurfaceShader::new("assets/materials/wet.material.json", code)
            .check()
            .unwrap_err();
        // The missing `;` is found where `return` starts, on line 4.
        assert_eq!(error.line, Some(4), "{error}");
        assert!(
            error
                .to_string()
                .starts_with("The shader of the material `assets/materials/wet.material.json` has an error on line 4: "),
            "{error}"
        );

        let wrong_type = "fn surface(input: SurfaceInput) -> Surface {\n    var out = \
                          standard_surface(input);\n\n    out.roughness = vec3<f32>(1.0);\n    \
                          return out;\n}\n";
        let error = SurfaceShader::new("typed", wrong_type).check().unwrap_err();
        assert_eq!(error.line, Some(4), "{error}");
        assert!(error.message.contains("`vec3<f32>(1.0)`"), "{error}");

        let missing = SurfaceShader::new("missing", "fn other() {}\n")
            .check()
            .unwrap_err();
        assert_eq!(missing.line, None, "{missing}");
        assert!(missing.message.contains("fn surface"), "{missing}");
    }

    #[test]
    fn the_same_code_has_the_same_key() {
        let a = SurfaceShader::new(
            "a",
            "fn surface(i: SurfaceInput) -> Surface { return standard_surface(i); }",
        );
        let b = SurfaceShader::new("b", a.code());
        assert_eq!(a.key(), b.key());
        assert_ne!(a.key(), SurfaceShader::new("a", "").key());
    }
}
