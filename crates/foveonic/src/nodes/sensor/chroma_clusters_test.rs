//! Checks metadata-derived cluster support and ordered template matching.
use super::*;
#[test]
fn empty_catalogue_is_valid() {
    assert!(Prepared::new([64, 64], vec![], 0, 25)
        .unwrap()
        .regions
        .is_empty());
}

fn independent_and_overlapping_regions() -> Prepared {
    let clusters = [[20, 20], [21, 20], [45, 45], [46, 45]]
        .map(|origin| Cluster {
            severity: 0,
            extent: [2, 2],
            origin,
            margin: 5,
            points: vec![[0, 0, 7]],
        })
        .into();
    Prepared::new([64, 64], clusters, 0, 5).unwrap()
}

#[test]
fn overlapping_search_regions_retain_catalogue_order() {
    let prepared = independent_and_overlapping_regions();
    assert_eq!(prepared.batches, vec![[0, 2], [2, 2]]);
    assert_eq!(
        prepared
            .regions
            .iter()
            .map(|r| r.rect[0])
            .collect::<Vec<_>>(),
        vec![14, 39, 15, 40]
    );
}

#[test]
#[ignore = "requires GPU"]
fn parallel_cluster_regions_match_serial_order_exactly() {
    let parallel = independent_and_overlapping_regions();
    let mut serial = parallel.clone();
    serial.regions.sort_by_key(|r| (r.rect[1], r.rect[0]));
    serial.batches = (0..serial.regions.len() as u32).map(|i| [i, 1]).collect();
    let values: Vec<f32> = (0..64 * 64 * 3)
        .map(|i| ((i * 73 + i * i * 7) % 4093) as f32 * 0.125)
        .collect();
    let gpu = Gpu::new().unwrap();
    let input = gpu.upload("Cluster dependency input", &values).unwrap();
    let mask = gpu
        .upload("Cluster dependency mask", &vec![0u32; 4096])
        .unwrap();
    let run = |prepared: &Prepared| {
        let output = apply(&gpu, &input, &mask, prepared, &mut Default::default()).unwrap();
        gpu.readback(&output).unwrap()
    };
    assert_eq!(run(&parallel), run(&serial));
}
