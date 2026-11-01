//! Estimates distinct native row and column bands and applies their selected strengths on GPU.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    noise::Parameters,
    profile::Profile,
    recipe::Key,
    render::{BandingAxisControls, MerrillBandingControls},
    source::Result,
};

const BLOCK_SIZE: u32 = 40;
const SEAM_SUPPORT: u32 = 5;
const WORKGROUP: u32 = 8;
const DELTA_WORKGROUP: u32 = 64;
// Fine, Coarse and a reflected Fine estimate for the Coarse difference.
const DELTA_SLOTS: usize = 3;
const COMPONENT_BYTES: u64 = 12;
const REFERENCE_FINE_WIDTH: u32 = 27;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BandParams {
    active_area: [u32; 4],
    sizes: [u32; 4],
    strengths: [f32; 4],
    axis: u32,
    estimate_slot: u32,
    reflect_samples: u32,
    reference_fine_boundary: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct Prepared {
    params: Parameters,
    active: [u32; 4],
    controls: MerrillBandingControls,
    native_extent: [u32; 2],
}

impl Prepared {
    pub(crate) fn new(
        params: Parameters,
        active: [u32; 4],
        controls: MerrillBandingControls,
    ) -> Result<Self> {
        controls.validate()?;
        if params.source[..2].contains(&0)
            || params.source[2..].contains(&0)
            || params.window[2..].contains(&0)
            || !params.column[0].is_finite()
            || params.column[0] <= 0.
            || params.model.iter().flatten().any(|v| !v.is_finite())
            || (0..2).any(|axis| {
                !params.source[axis].is_multiple_of(params.source[axis + 2])
                    || params.tile[axis]
                        != params.source[axis] / params.source[axis + 2] + 2 * params.tile[2]
                    || params.tile[axis] < BLOCK_SIZE
                    || active[axis] + 1 >= active[axis + 2]
                    || active[axis + 2] > params.source[axis]
                    || params.column_area[axis] >= params.column_area[axis + 2]
                    || params.column_area[axis + 2] > params.source[axis]
                    || params.column_area[axis] >= params.tile[axis]
                    || params.source[axis] - params.column_area[axis + 2] >= params.tile[axis]
                    || params.window[axis] + params.window[axis + 2] > params.source[axis + 2]
            })
        {
            return Err("invalid directional banding geometry or noise model".into());
        }
        for axis in [controls.columns, controls.rows] {
            if axis
                .bands
                .iter()
                .any(|band| band.size / 2 + SEAM_SUPPORT > params.tile[2])
            {
                return Err("directional banding width exceeds processing tile context".into());
            }
        }
        Ok(Self {
            params,
            active,
            controls,
            native_extent: [params.source[0], params.source[1]],
        })
    }

    pub(crate) fn with_native_extent(mut self, native: [u32; 2]) -> Result<Self> {
        if native.contains(&0) || (0..2).any(|axis| self.params.source[axis] > native[axis]) {
            return Err("invalid native directional banding extent".into());
        }
        self.native_extent = native;
        Ok(self)
    }

    fn working_controls(&self, direction: usize) -> BandingAxisControls {
        let mut controls = if direction == 0 {
            self.controls.columns
        } else {
            self.controls.rows
        };
        // Stored support is in native pixels. A reduced preview rounds the
        // radius to the nearest working pixel. Equal working widths are valid.
        for band in &mut controls.bands {
            let numerator = u64::from(band.size / 2) * u64::from(self.params.source[direction]);
            let denominator = u64::from(self.native_extent[direction]);
            let radius = ((numerator + denominator / 2) / denominator).max(1) as u32;
            band.size = radius * 2 + 1;
        }
        controls
    }

    fn uses_reference_fine_boundary(&self, direction: usize) -> bool {
        self.extent() == self.native_extent
            && self.working_controls(direction).bands[0].size == REFERENCE_FINE_WIDTH
    }

    pub(crate) fn params(&self) -> Parameters {
        self.params
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.source[0], self.params.source[1]]
    }
    pub(crate) fn key(&self) -> Key {
        let mut key = Key::pod(&self.params)
            .with(&self.active)
            .with(&self.native_extent)
            .with(&u32::from(self.controls.enabled));
        for axis in [
            self.controls.columns,
            self.controls.rows,
            self.working_controls(0),
            self.working_controls(1),
        ] {
            for band in axis.bands {
                key = key.with(&band.size).with(&band.strength);
            }
        }
        key
    }

    fn uniform(&self, axis: usize, controls: BandingAxisControls, scale: usize) -> BandParams {
        BandParams {
            active_area: self.active,
            sizes: [
                controls.bands[0].size,
                controls.bands[1].size,
                controls.bands[0].size,
                0,
            ],
            strengths: [
                controls.bands[0].strength,
                controls.bands[1].strength,
                0.,
                0.,
            ],
            axis: axis as u32,
            estimate_slot: scale as u32,
            reflect_samples: u32::from(scale != 0 || !self.uses_reference_fine_boundary(axis)),
            reference_fine_boundary: u32::from(self.uses_reference_fine_boundary(axis)),
        }
    }
}

fn required_estimates(
    controls: BandingAxisControls,
    reference_fine_boundary: bool,
) -> [bool; DELTA_SLOTS] {
    let [fine, coarse] = controls.bands.map(|band| band.strength != 0.);
    [
        fine || (coarse && !reference_fine_boundary),
        coarse,
        coarse && reference_fine_boundary,
    ]
}

fn apply_axis(
    gpu: &Gpu,
    input: ReadGuard,
    prepared: &Prepared,
    direction: usize,
    controls: BandingAxisControls,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let needed = required_estimates(controls, prepared.uses_reference_fine_boundary(direction));
    if !needed.into_iter().any(|v| v) {
        return Ok(input);
    }
    let p = prepared.params;
    let pixel_bytes = p.atlas_pixels() * COMPONENT_BYTES;
    let blocks = p.tile[1 - direction] / BLOCK_SIZE;
    let delta_pixels = u64::from(blocks) * u64::from(p.tile[direction]) * u64::from(p.tile_count());
    let mut smooth = gpu.storage("Directional band neighborhoods", pixel_bytes)?;
    let mut deltas = gpu.storage(
        "Directional cumulative band estimates",
        delta_pixels * COMPONENT_BYTES * DELTA_SLOTS as u64,
    )?;
    let mut output = gpu.storage("Directional band correction tiles", pixel_bytes)?;
    let noise_uniform = gpu.uniform(&p);
    let lookup = gpu.upload(
        "Directional band range weights",
        &super::reconstruction::tables(p),
    )?;
    let groups = p.atlas_extent().map(|v| v.div_ceil(WORKGROUP));
    let mut trace = gpu.trace(
        profile.mode.gpu(),
        needed.into_iter().filter(|v| *v).count() as u32 * 2 + 1,
    );
    for (scale, needed) in needed.into_iter().enumerate() {
        if !needed {
            continue;
        }
        let uniform = gpu.uniform(&prepared.uniform(direction, controls, scale));
        gpu.dispatch(
            Kernel::DirectionalBandingSmooth,
            &mut [
                (0, Read(&input)),
                (1, Read(&noise_uniform)),
                (2, Write(smooth.write())),
                (5, Read(&lookup)),
                (6, Read(&uniform)),
            ],
            groups,
            &mut trace,
        );
        gpu.dispatch(
            Kernel::DirectionalBandingDeltas,
            &mut [
                (0, Read(&input)),
                (1, Read(&noise_uniform)),
                (2, Read(&smooth)),
                (3, Write(deltas.write())),
                (6, Read(&uniform)),
            ],
            [
                p.tile[direction].div_ceil(DELTA_WORKGROUP),
                blocks * p.tile_count(),
            ],
            &mut trace,
        );
    }
    let uniform = gpu.uniform(&prepared.uniform(direction, controls, 0));
    gpu.dispatch(
        Kernel::DirectionalBandingApply,
        &mut [
            (0, Read(&input)),
            (1, Read(&noise_uniform)),
            (3, Read(&deltas)),
            (4, Write(output.write())),
            (6, Read(&uniform)),
        ],
        groups,
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    if input.size()
        != u64::from(prepared.extent()[0]) * u64::from(prepared.extent()[1]) * COMPONENT_BYTES
    {
        return Err("directional banding input does not match sensor extent".into());
    }
    let tiles = super::reconstruction::raw_tiles(gpu, input, prepared.params, profile)?;
    if !prepared.controls.enabled {
        return Ok(tiles);
    }
    let columns = apply_axis(
        gpu,
        tiles,
        prepared,
        0,
        prepared.working_controls(0),
        profile,
    )?;
    apply_axis(
        gpu,
        columns,
        prepared,
        1,
        prepared.working_controls(1),
        profile,
    )
}

#[cfg(test)]
#[path = "directional_banding_test.rs"]
mod tests;
