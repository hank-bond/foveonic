//! Checks stable cluster priorities and native mono patch-search results.
use super::*;
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
