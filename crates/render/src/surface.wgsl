// The standard material's surface: an item without a surface shader of its
// own. A material's custom shader takes this file's place after
// shader.wgsl (see renderer/surface_shaders.rs).
fn surface(input: SurfaceInput) -> Surface {
    return standard_surface(input);
}
