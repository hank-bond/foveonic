// Independently expressed six-coefficient response from reference B&W research.
// Primaries and complements are in the reference linear working coordinates.
const REFERENCE_LUMA: vec3<f32> = vec3(0.3328, 0.6121, 0.0551);
const REFERENCE_COMPLEMENTS: vec3<f32> = vec3(0.6672, 0.3879, 0.9449);
const DARK_RESPONSE_KNEE: f32 = 0.125;
const DARK_RESPONSE_OFFSET: f32 = 0.03125;
@compute @workgroup_size(8, 8)
fn color_sensitivity_map(@builtin(global_invocation_id) id: vec3<u32>) {
    if !valid(id) { return; }
    let i = id.y * p.shape.x + id.x;
    let v = rgb(i);
    // Negative guide components cannot classify a visible subject color.
    let c = max(vec3(dot(v, p.guide_matrix[0].xyz), dot(v, p.guide_matrix[1].xyz),
        dot(v, p.guide_matrix[2].xyz)), vec3(0.));
    let lo = min(c.x, min(c.y, c.z));
    let hi = max(c.x, max(c.y, c.z));
    let middle = c.x + c.y + c.z - lo - hi;
    var lowest = 0u;
    var highest = 0u;
    if c.y < c.x { lowest = 1u; }
    if c.z < c[lowest] { lowest = 2u; }
    if c.y > c.x { highest = 1u; }
    if c.z > c[highest] { highest = 2u; }
    // UI order: Red, Yellow, Green, Cyan, Blue, Magenta.
    let primary = REFERENCE_LUMA + vec3(p.hues[0].x, p.hues[0].z, p.hues[1].x);
    let secondary = REFERENCE_COMPLEMENTS + vec3(p.hues[0].w, p.hues[1].y, p.hues[0].y);
    let neutral = dot(c, REFERENCE_LUMA);
    var desired = lo + (middle - lo) * secondary[lowest] + (hi - middle) * primary[highest];
    if desired < neutral * DARK_RESPONSE_KNEE {
        // Rational soft toe tangent to the linear response at Y/8. The offset
        // sets its dark extension, avoiding a hard clamp under strong filtering.
        let response = desired / neutral;
        let span = DARK_RESPONSE_KNEE + DARK_RESPONSE_OFFSET;
        desired = max(0., neutral * (span * span /
            (2. * DARK_RESPONSE_KNEE + DARK_RESPONSE_OFFSET - response) - DARK_RESPONSE_OFFSET));
    }
    let floor_value = p.values.x * SIGNAL_FLOOR;
    let gain = clamp(log2(max(desired, floor_value) / max(neutral, floor_value)),
        -MAX_TRANSFER_STOPS, MAX_TRANSFER_STOPS);
    dst[i] = gain * smoothstep(floor_value, SHADOW_FADE * floor_value, neutral);
}

