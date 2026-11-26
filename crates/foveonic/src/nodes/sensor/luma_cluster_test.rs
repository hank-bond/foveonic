//! Checks stable cluster priorities and native mono patch-search results.
use super::*;

fn test_clusters() -> Vec<Cluster> {
    [([16, 16], 5), ([16, 25], 8), ([48, 48], 5), ([16, 34], 5)]
        .map(|(origin, margin)| Cluster {
            origin,
            extent: [1, 1],
            margin,
            points: vec![[0, 0]],
        })
        .into()
}

#[test]
fn batches_preserve_dependencies_through_the_donor_patch_halo() {
    let p = Prepared::new([64, 64], test_clusters(), 8).unwrap();
    assert_eq!(p.batches, [[0, 2], [2, 1], [3, 1]]);
    let order: Vec<_> = p.jobs.iter().map(|job| job.first_point).collect();
    assert_eq!(order, [0, 2, 1, 3]);
}

#[test]
#[ignore = "requires a GPU"]
fn batched_repair_matches_catalogue_order() {
    let gpu = Gpu::new().unwrap();
    let samples: Vec<f32> = (0..4096_u32)
        .map(|i| ((i * 17 + i / 11) % 503) as f32)
        .collect();
    let input = gpu.upload("Luma cluster input", &samples).unwrap();
    let mut profile = Profile::default();
    let p = Prepared::new([64, 64], test_clusters(), 8).unwrap();
    let batched = apply(&gpu, &input, &p, &mut profile).unwrap();
    let mut serial = input;
    for cluster in test_clusters() {
        let p = Prepared::new([64, 64], vec![cluster], 8).unwrap();
        serial = apply(&gpu, &serial, &p, &mut profile).unwrap();
    }
    assert_eq!(
        gpu.readback(&batched).unwrap(),
        gpu.readback(&serial).unwrap()
    );
}

#[test]
fn rejects_clipped_or_duplicate_support() {
    let c = Cluster {
        origin: [12, 12],
        extent: [2, 2],
        margin: 5,
        points: vec![[0, 0], [0, 0]],
    };
    assert!(Prepared::new([32, 32], vec![c], 5).is_err());
}
#[test]
#[ignore = "requires a GPU and LUMA_CLUSTER_REFERENCE"]
fn bounded_native_clusters_match() {
    let dir = std::path::PathBuf::from(std::env::var("LUMA_CLUSTER_REFERENCE").unwrap());
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("cases.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let points = case["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| [p[0].as_u64().unwrap() as u32, p[1].as_u64().unwrap() as u32])
            .collect();
        let c = Cluster {
            origin: [12, 12],
            extent: [2, 2],
            margin: 5,
            points,
        };
        let p = Prepared::new([32, 32], vec![c], 5).unwrap();
        let input = std::fs::read(dir.join(format!("{name}-input.bin"))).unwrap();
        let input = gpu
            .upload(
                "Luma cluster fixture",
                bytemuck::cast_slice::<u8, f32>(&input),
            )
            .unwrap();
        let output = apply(&gpu, &input, &p, &mut Profile::default()).unwrap();
        let expected = std::fs::read(dir.join(format!("{name}-output.bin"))).unwrap();
        assert_eq!(gpu.readback(&output).unwrap(), expected, "{name}");
    }
}
