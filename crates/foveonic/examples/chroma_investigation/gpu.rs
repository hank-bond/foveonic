//! Runs test-only local-reference shaders while preserving tracked GPU buffer ownership.

use super::{Binding, Gpu, ReadGuard};
use crate::source::Result;

pub(crate) struct Kernels {
    baseline: wgpu::ComputePipeline,
    local: wgpu::ComputePipeline,
    summarize: wgpu::ComputePipeline,
    presentation: wgpu::ComputePipeline,
}
impl Kernels {
    pub(crate) fn new(gpu: &Gpu) -> Self {
        let code = format!(
            "{}\n{}\n{}",
            include_str!("../../src/nodes/color/noise_common.wgsl"),
            include_str!("baseline_texture.wgsl"),
            include_str!("local.wgsl")
        );
        let module = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Local reference investigation"),
                source: wgpu::ShaderSource::Wgsl(code.into()),
            });
        let pipeline = |entry| {
            gpu.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(entry),
                    layout: None,
                    module: &module,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    cache: None,
                })
        };
        let presentation_module = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Local reference presentation"),
                source: wgpu::ShaderSource::Wgsl(include_str!("presentation.wgsl").into()),
            });
        let presentation = gpu
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Local reference presentation"),
                layout: None,
                module: &presentation_module,
                entry_point: Some("display_crops"),
                compilation_options: Default::default(),
                cache: None,
            });
        Self {
            baseline: pipeline("mix_texture"),
            local: pipeline("local_reference"),
            summarize: pipeline("summarize_reference"),
            presentation,
        }
    }
    pub(crate) fn baseline(
        &self,
        gpu: &Gpu,
        inputs: crate::nodes::color::texture::Inputs<'_>,
        noise: &ReadGuard,
        texture: &ReadGuard,
        extent: [u32; 2],
    ) -> Result<ReadGuard> {
        let mut output = gpu.storage(
            "Pre-adoption texture baseline",
            u64::from(extent[0]) * u64::from(extent[1]) * 12,
        )?;
        Self::dispatch(
            gpu,
            &self.baseline,
            &mut [
                (0, Binding::Read(inputs.raw)),
                (1, Binding::Read(noise)),
                (2, Binding::Write(output.write())),
                (3, Binding::Read(inputs.base.ok_or("baseline layers")?)),
                (4, Binding::Read(inputs.ratio.ok_or("baseline ratio")?)),
                (5, Binding::Read(inputs.ratio.ok_or("baseline ratio")?)),
                (6, Binding::Read(inputs.ratio.ok_or("baseline ratio")?)),
                (7, Binding::Read(texture)),
            ],
            extent.map(|v| v.div_ceil(8)),
        );
        Ok(output)
    }
    fn dispatch(
        gpu: &Gpu,
        pipeline: &wgpu::ComputePipeline,
        bindings: &mut [(u32, Binding<'_>)],
        groups: [u32; 2],
    ) {
        let entries: Vec<_> = bindings
            .iter()
            .map(|(binding, buffer)| wgpu::BindGroupEntry {
                binding: *binding,
                resource: buffer.raw().as_entire_binding(),
            })
            .collect();
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Local reference inputs"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        });
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(groups[0], groups[1], 1);
        }
        gpu.submit(encoder);
        for (_, binding) in bindings {
            binding.submitted("local reference investigation");
        }
    }
    pub(crate) fn present(
        &self,
        gpu: &Gpu,
        input: &ReadGuard,
        params: &ReadGuard,
        extent: [u32; 2],
    ) -> Result<ReadGuard> {
        let mut output = gpu.storage(
            "Local reference display",
            u64::from(extent[0]) * u64::from(extent[1]) * 12,
        )?;
        Self::dispatch(
            gpu,
            &self.presentation,
            &mut [
                (0, Binding::Read(input)),
                (1, Binding::Read(params)),
                (2, Binding::Write(output.write())),
            ],
            [extent[0].div_ceil(8), extent[1].div_ceil(8) * 3],
        );
        Ok(output)
    }
    pub(crate) fn apply(
        &self,
        gpu: &Gpu,
        inputs: Inputs<'_>,
        uniforms: Uniforms<'_>,
        extent: [u32; 2],
    ) -> Result<(ReadGuard, ReadGuard)> {
        let mut output = gpu.storage(
            "Local reference result",
            u64::from(extent[0]) * u64::from(extent[1]) * 12,
        )?;
        let mut mask = gpu.storage(
            "Local reference confidence",
            u64::from(extent[0]) * u64::from(extent[1]) * 4,
        )?;
        Self::dispatch(
            gpu,
            &self.local,
            &mut [
                (0, Binding::Read(inputs.raw)),
                (1, Binding::Read(uniforms.noise)),
                (2, Binding::Write(output.write())),
                (3, Binding::Read(inputs.base)),
                (4, Binding::Read(inputs.ratio)),
                (7, Binding::Read(uniforms.texture)),
                (8, Binding::Read(uniforms.study)),
                (9, Binding::Read(inputs.original)),
                (10, Binding::Write(mask.write())),
            ],
            extent.map(|v| v.div_ceil(8)),
        );
        Ok((output, mask))
    }
    pub(crate) fn summarize(
        &self,
        gpu: &Gpu,
        original: &ReadGuard,
        corrected: &ReadGuard,
        mask: &ReadGuard,
        noise: &ReadGuard,
        study: &ReadGuard,
    ) -> Result<Vec<[f32; 4]>> {
        let count = (mask.size() / 4) as u32;
        let groups = count.div_ceil(256);
        let mut result = gpu.storage("Local reference summary", u64::from(groups) * 48)?;
        Self::dispatch(
            gpu,
            &self.summarize,
            &mut [
                (1, Binding::Read(noise)),
                (8, Binding::Read(study)),
                (9, Binding::Read(original)),
                (10, Binding::Read(mask)),
                (11, Binding::Read(corrected)),
                (12, Binding::Write(result.write())),
            ],
            [groups.min(256), groups.div_ceil(256)],
        );
        Ok(bytemuck::cast_slice::<u8, [f32; 4]>(&gpu.readback(&result)?).to_vec())
    }
}
pub(crate) struct Inputs<'a> {
    pub raw: &'a ReadGuard,
    pub base: &'a ReadGuard,
    pub ratio: &'a ReadGuard,
    pub original: &'a ReadGuard,
}
pub(crate) struct Uniforms<'a> {
    pub noise: &'a ReadGuard,
    pub texture: &'a ReadGuard,
    pub study: &'a ReadGuard,
}
