// The client supplies RGB colorants and chooses whether to limit its viewing gamut.
struct ColorTransform {
    to_gamut: array<vec4<f32>, 3>,
    from_gamut: array<vec4<f32>, 3>,
    limit: vec4<u32>,
}
fn apply_color_matrix(rgb: vec3<f32>, rows: array<vec4<f32>, 3>) -> vec3<f32> {
    return vec3(dot(rows[0].xyz, rgb), dot(rows[1].xyz, rgb), dot(rows[2].xyz, rgb));
}
fn map_display_color(rgb: vec3<f32>, transform: ColorTransform) -> vec3<f32> {
    if transform.limit.x == 0u { return rgb; }
    let limited = clamp(apply_color_matrix(rgb, transform.to_gamut), vec3(0.), vec3(1.));
    return apply_color_matrix(limited, transform.from_gamut);
}
