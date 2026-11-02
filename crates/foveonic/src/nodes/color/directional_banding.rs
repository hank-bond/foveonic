//! Measures native Fine bands, fits axis-coherent residual Coarse offsets, and shares their tiles with previews.

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
const COMPONENT_BYTES: u64 = 12;
const REFERENCE_FINE_WIDTH: u32 = 27;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Axis {
    Columns,
    Rows,
}

impl Axis {
    fn index(self) -> usize {
        match self {
            Self::Columns => 0,
            Self::Rows => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Band {
    Fine,
    Coarse,
}

impl Band {
    fn index(self) -> usize {
        match self {
            Self::Fine => 0,
            Self::Coarse => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    Tiles,
    Retile,
    Window,
    Measure(Band, Axis),
    Correct(Band, Axis),
}

impl Stage {
    pub(crate) fn output(self) -> &'static str {
        match self {
            Self::Tiles => "banding.source-tiles",
            Self::Retile => "banding.preview-tiles",
            Self::Window => "banding.region-tiles",
            Self::Measure(Band::Fine, Axis::Columns) => "banding.fine-column-estimates",
            Self::Measure(Band::Fine, Axis::Rows) => "banding.fine-row-estimates",
            Self::Measure(Band::Coarse, Axis::Columns) => "banding.coarse-column-estimates",
            Self::Measure(Band::Coarse, Axis::Rows) => "banding.coarse-row-estimates",
            Self::Correct(Band::Fine, Axis::Columns) => "banding.fine-column-tiles",
            Self::Correct(Band::Fine, Axis::Rows) => "banding.fine-tiles",
            Self::Correct(Band::Coarse, Axis::Columns) => "banding.coarse-column-tiles",
            Self::Correct(Band::Coarse, Axis::Rows) => "column.tiles",
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Tiles => "prepare directional band tiles",
            Self::Retile => "prepare reduced band tiles",
            Self::Window => "select native band tiles",
            Self::Measure(Band::Fine, Axis::Columns) => "measure native fine column bands",
            Self::Measure(Band::Fine, Axis::Rows) => "measure native fine row bands",
            Self::Measure(Band::Coarse, Axis::Columns) => "measure native coarse column bands",
            Self::Measure(Band::Coarse, Axis::Rows) => "measure native coarse row bands",
            Self::Correct(Band::Fine, Axis::Columns) => "correct native fine column bands",
            Self::Correct(Band::Fine, Axis::Rows) => "correct native fine row bands",
            Self::Correct(Band::Coarse, Axis::Columns) => "correct native coarse column bands",
            Self::Correct(Band::Coarse, Axis::Rows) => "correct native coarse row bands",
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BandParams {
    active_area: [u32; 4],
    width: u32,
    axis: u32,
    kind: u32,
    reflect_samples: u32,
    strength: f32,
    padding: [u32; 3],
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

    pub(crate) fn key(&self, stage: Stage) -> Key {
        let mut key = Key::pod(&self.params)
            .with(&self.active)
            .with(&self.native_extent)
            .slice(stage.output().as_bytes());
        let (kind, axis) = match stage {
            Stage::Measure(kind, axis) | Stage::Correct(kind, axis) => (kind, axis),
            _ => return key,
        };
        let band = self.working_controls(axis.index()).bands[kind.index()];
        key = key.with(&band.size).with(&u32::from(self.controls.enabled));
        if matches!(stage, Stage::Measure(..)) {
            key.with(&u32::from(self.controls.enabled && band.strength != 0.))
        } else {
            key.with(&band.strength)
        }
    }
    fn uniform(&self, axis: usize, kind: Band) -> BandParams {
        let selected = self.working_controls(axis).bands[kind.index()];
        BandParams {
            active_area: self.active,
            width: selected.size,
            axis: axis as u32,
            kind: kind.index() as u32,
            reflect_samples: u32::from(
                kind == Band::Coarse || !self.uses_reference_fine_boundary(axis),
            ),
            strength: if self.controls.enabled {
                selected.strength
            } else {
                0.
            },
            padding: [0; 3],
        }
    }
}

fn block_capacity(p: Parameters, direction: usize, kind: Band) -> u32 {
    match kind {
        Band::Fine => p.tile[1 - direction] / BLOCK_SIZE,
        Band::Coarse => p.tile[1 - direction].div_ceil(BLOCK_SIZE) + 1,
    }
}

fn delta_bytes(p: Parameters, direction: usize, kind: Band) -> u64 {
    u64::from(block_capacity(p, direction, kind))
        * u64::from(p.tile[direction])
        * u64::from(p.tile_count())
        * COMPONENT_BYTES
}

fn measure_axis(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    direction: usize,
    kind: Band,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let p = prepared.params;
    let band = prepared.uniform(direction, kind);
    if band.strength == 0. {
        return gpu.storage("Inactive directional band estimates", COMPONENT_BYTES);
    }
    if kind == Band::Coarse && p.window != p.full_frame().window {
        return Err("Coarse band measurement requires full native context".into());
    }
    let mut smooth = gpu.storage(
        "Directional neighborhoods",
        p.atlas_pixels() * COMPONENT_BYTES,
    )?;
    let mut raw = gpu.storage("Measured band offsets", delta_bytes(p, direction, kind))?;
    let noise = gpu.uniform(&p);
    let uniform = gpu.uniform(&band);
    let lookup = gpu.upload(
        "Directional range weights",
        &super::reconstruction::tables(p),
    )?;
    let mut trace = gpu.trace(profile.mode.gpu(), if kind == Band::Coarse { 3 } else { 2 });
    gpu.dispatch(
        Kernel::DirectionalBandingSmooth,
        &mut [
            (0, Read(input)),
            (1, Read(&noise)),
            (2, Write(smooth.write())),
            (5, Read(&lookup)),
            (6, Read(&uniform)),
        ],
        p.atlas_extent().map(|v| v.div_ceil(WORKGROUP)),
        &mut trace,
    );
    let groups = [
        p.tile[direction].div_ceil(DELTA_WORKGROUP),
        block_capacity(p, direction, kind) * p.tile_count(),
    ];
    gpu.dispatch(
        Kernel::DirectionalBandingDeltas,
        &mut [
            (0, Read(input)),
            (1, Read(&noise)),
            (2, Read(&smooth)),
            (3, Write(raw.write())),
            (6, Read(&uniform)),
        ],
        groups,
        &mut trace,
    );
    let result = if kind == Band::Coarse {
        let mut coherent = gpu.storage("Axis-coherent Coarse offsets", raw.size())?;
        gpu.dispatch(
            Kernel::DirectionalBandingFit,
            &mut [
                (1, Read(&noise)),
                (3, Read(&raw)),
                (4, Write(coherent.write())),
                (6, Read(&uniform)),
            ],
            groups,
            &mut trace,
        );
        coherent
    } else {
        raw
    };
    gpu.finish_trace(&trace, profile)?;
    Ok(result)
}

fn correct_axis(
    gpu: &Gpu,
    input: &ReadGuard,
    estimates: &ReadGuard,
    prepared: &Prepared,
    direction: usize,
    kind: Band,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let p = prepared.params;
    let band = prepared.uniform(direction, kind);
    if band.strength == 0. {
        return Ok(input.clone());
    }
    if estimates.size() != delta_bytes(p, direction, kind) {
        return Err("Band estimates do not match native tile geometry".into());
    }
    let mut output = gpu.storage(
        "Corrected native band tiles",
        p.atlas_pixels() * COMPONENT_BYTES,
    )?;
    let noise = gpu.uniform(&p);
    let uniform = gpu.uniform(&band);
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::DirectionalBandingApply,
        &mut [
            (0, Read(input)),
            (1, Read(&noise)),
            (3, Read(estimates)),
            (4, Write(output.write())),
            (6, Read(&uniform)),
        ],
        p.atlas_extent().map(|v| v.div_ceil(WORKGROUP)),
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
// Preserve every native tile's halo and phase when packing a regional atlas.
// Re-projecting and re-tiling here would blend the overlap twice.

fn select_window(gpu: &Gpu, input: &ReadGuard, p: Parameters) -> Result<ReadGuard> {
    let full = p.full_frame();
    if input.size() != full.atlas_pixels() * COMPONENT_BYTES {
        return Err("Native band tiles do not match the source geometry".into());
    }
    if p.window == full.window {
        return Ok(input.clone());
    }
    let tile_bytes = u64::from(p.tile[0]) * u64::from(p.tile[1]) * COMPONENT_BYTES;
    let mut output = gpu.storage(
        "Regional native band tiles",
        p.atlas_pixels() * COMPONENT_BYTES,
    )?;
    let ranges = (0..p.tile_count()).map(|tile| {
        let x = p.window[0] + tile % p.window[2];
        let y = p.window[1] + tile / p.window[2];
        [
            u64::from(y * p.source[2] + x) * tile_bytes,
            u64::from(tile) * tile_bytes,
            tile_bytes,
        ]
    });
    gpu.copy_ranges(input, output.write(), ranges);
    Ok(output)
}

pub(crate) fn apply_stage(
    gpu: &Gpu,
    input: &ReadGuard,
    estimates: Option<&ReadGuard>,
    prepared: &Prepared,
    stage: Stage,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let p = prepared.params;
    if matches!(stage, Stage::Tiles | Stage::Retile) {
        if input.size() != u64::from(p.source[0]) * u64::from(p.source[1]) * COMPONENT_BYTES {
            return Err("Banding input does not match sensor extent".into());
        }
        return super::reconstruction::raw_tiles(gpu, input, p, profile);
    }
    if stage == Stage::Window {
        return select_window(gpu, input, p);
    }
    if input.size() != p.atlas_pixels() * COMPONENT_BYTES {
        return Err("Band input does not match tile geometry".into());
    }
    match stage {
        Stage::Measure(kind, axis) => {
            measure_axis(gpu, input, prepared, axis.index(), kind, profile)
        }
        Stage::Correct(kind, axis) => correct_axis(
            gpu,
            input,
            estimates.ok_or("missing band estimates")?,
            prepared,
            axis.index(),
            kind,
            profile,
        ),
        _ => unreachable!(),
    }
}

#[cfg(test)]
#[path = "directional_banding_test.rs"]
mod tests;
