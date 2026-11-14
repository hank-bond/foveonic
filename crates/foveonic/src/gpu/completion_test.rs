//! Checks submitted-work completion and readback without a blocking device poll.

use wgpu::util::DeviceExt;

#[test]
#[ignore = "requires a GPU, run serialized"]
fn completed_submission_preserves_copy_results_and_allows_later_work() {
    let gpu = crate::gpu::Gpu::new().unwrap();
    let values = [17_u32, 42, 65535, 1];
    let expected: &[u8] = bytemuck::cast_slice(&values);
    let source = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Completion input"),
            contents: expected,
            usage: wgpu::BufferUsages::COPY_SRC,
        });
    let target = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Completion readback"),
        size: std::mem::size_of_val(&values) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    for _ in 0..2 {
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&source, 0, &target, 0, target.size());
        gpu.queue.submit([encoder.finish()]);
        gpu.wait_for_submitted_work().unwrap();
        crate::gpu::map_readback(&gpu.device, &target).unwrap();
        assert_eq!(&*target.slice(..).get_mapped_range(), expected);
        target.unmap();
    }
}
