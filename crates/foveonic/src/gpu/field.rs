//! Compiles optional color-field kernels lazily and reflects their exact guarded resource access.
use super::{Gpu, Kernel};
use crate::source::Result;
use wgpu::naga::{
    front::wgsl,
    valid::{Capabilities, GlobalUse, ValidationFlags, Validator},
};

pub(super) struct FieldKernels {
    pipelines: Vec<wgpu::ComputePipeline>,
    pub(super) bindings: Vec<(u32, u32)>,
}
impl Gpu {
    pub(crate) fn prepare_color_field(&self) -> Result<()> {
        let result = self
            .field
            .get_or_init(|| compile(&self.device).map_err(|e| e.to_string()));
        result.as_ref().map(|_| ()).map_err(|e| e.clone().into())
    }
    pub(super) fn field_data(&self) -> &FieldKernels {
        self.field
            .get()
            .expect("color-field kernels prepared")
            .as_ref()
            .expect("color-field compilation succeeded")
    }
    pub(super) fn pipeline(&self, kernel: Kernel) -> &wgpu::ComputePipeline {
        let index = kernel as usize;
        if index >= Kernel::FieldPrepare as usize {
            &self.field_data().pipelines[index - Kernel::FieldPrepare as usize]
        } else {
            &self.pipelines[index]
        }
    }
    #[cfg(test)]
    pub(super) fn write_mask(&self, kernel: Kernel) -> u32 {
        if kernel as usize >= Kernel::FieldPrepare as usize {
            self.field_data().bindings[kernel as usize - Kernel::FieldPrepare as usize].1
        } else {
            self.shader_writes[kernel as usize]
        }
    }
}
fn compile(device: &wgpu::Device) -> Result<FieldKernels> {
    let field = format!(
        "{}\n{}",
        include_str!("../nodes/sensor/color_field_math.wgsl"),
        include_str!("../nodes/sensor/color_field.wgsl")
    );
    let solver = include_str!("../nodes/sensor/color_field_solver.wgsl");
    let groups = [
        (
            field.as_str(),
            &[
                "prepare_source",
                "mark_repairs",
                "native_equations",
                "commit_tone",
                "collect_grid",
                "connect_grid",
                "union_grid",
                "region_peaks",
                "peak_indices",
                "condition_grid",
                "propagate_distance",
                "color_equations",
                "commit_color",
                "apply_field",
                "compact_rank",
                "compact_prefix",
                "compact_scatter",
            ][..],
        ),
        (
            solver,
            &[
                "initialize",
                "initialize_state",
                "map_direction",
                "alpha",
                "step",
                "beta",
                "update_direction",
                "local_forward",
                "local_backward",
            ][..],
        ),
    ];
    let mut result = FieldKernels {
        pipelines: Vec::new(),
        bindings: Vec::new(),
    };
    for (code, entries) in groups {
        let module = wgsl::parse_str(code).map_err(|e| e.emit_to_string(code))?;
        let info = Validator::new(ValidationFlags::all(), Capabilities::all())
            .validate(&module)
            .map_err(|e| e.emit_to_string(code))?;
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Color-field optional kernels"),
            source: wgpu::ShaderSource::Wgsl(code.into()),
        });
        for &entry in entries {
            let ep = module
                .entry_points
                .iter()
                .position(|p| p.name == entry)
                .expect("registered color-field entry");
            let function = info.get_entry_point(ep);
            let mut used = 0;
            let mut writes = 0;
            for (handle, variable) in module.global_variables.iter() {
                if let Some(binding) = &variable.binding {
                    if !function[handle].is_empty() {
                        used |= 1 << binding.binding;
                    }
                    if function[handle].intersects(GlobalUse::WRITE | GlobalUse::ATOMIC) {
                        writes |= 1 << binding.binding;
                    }
                }
            }
            result.bindings.push((used, writes));
            result.pipelines.push(device.create_compute_pipeline(
                &wgpu::ComputePipelineDescriptor {
                    label: Some(entry),
                    layout: None,
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    cache: None,
                },
            ));
        }
        if let Some(error) = pollster::block_on(device.pop_error_scope()) {
            return Err(error.into());
        }
    }
    Ok(result)
}
