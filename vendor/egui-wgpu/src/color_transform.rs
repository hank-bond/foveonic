//! Carries client-supplied display transforms shared by native images and photographic callbacks.

/// A linear RGB gamut limit. Matrices and policy belong to the application.
/// Matrices share a reference white. RGB is mapped into the chosen gamut,
/// limited to 0..1 when enabled, then mapped back before transfer encoding.
/// Rows are padded to vec4 alignment for direct uniform-buffer use.
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct ColorTransform {
    pub to_gamut: [[f32; 4]; 3],
    pub from_gamut: [[f32; 4]; 3],
    pub limit: [u32; 4],
}
impl ColorTransform {
    pub fn new(to_gamut: [[f32; 4]; 3], from_gamut: [[f32; 4]; 3]) -> Self {
        Self {
            to_gamut,
            from_gamut,
            limit: [1, 0, 0, 0],
        }
    }
}
impl Default for ColorTransform {
    fn default() -> Self {
        const IDENTITY: [[f32; 4]; 3] = [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]];
        Self {
            to_gamut: IDENTITY,
            from_gamut: IDENTITY,
            limit: [0; 4],
        }
    }
}

/// Shared matrix conversion for native images and photographic callbacks.
pub const COLOR_TRANSFORM_SHADER: &str = include_str!("color_transform.wgsl");
