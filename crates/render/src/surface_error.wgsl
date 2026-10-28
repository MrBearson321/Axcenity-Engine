// The surface of an item whose material's shader has an error: glowing
// magenta and black checks half a stud across, so it's seen at once.
fn surface(input: SurfaceInput) -> Surface {
    let cell = floor(input.world_position * 2.0);
    let odd = (i32(cell.x + cell.y + cell.z) & 1) == 1;
    var out: Surface;
    out.base_color = vec3<f32>(0.0);
    out.metalness = 0.0;
    out.roughness = 1.0;
    out.normal = input.normal;
    out.emissive = select(vec3<f32>(1.0, 0.0, 1.0), vec3<f32>(0.05), odd);
    out.opacity = 1.0;
    return out;
}
