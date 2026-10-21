//! Checks bounded map ownership, branch selection and reference blend arithmetic.

use super::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "foveonic-blue-maps-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn write(path: &Path, extent: [u16; 2], values: &[f32]) {
    let mut data: Vec<_> = values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    data.extend(extent[1].to_le_bytes());
    data.extend(extent[0].to_le_bytes());
    std::fs::write(path, data).unwrap();
}

#[test]
fn directory_owns_maps_and_missing_known_resources_are_errors() {
    let directory = Directory::new();
    for index in 1..=6 {
        write(
            &directory.0.join(format!("C81_{index}")),
            [2, 2],
            &[index as f32; 4],
        );
    }
    let resources = ResourceDirectory::new(&directory.0).unwrap();
    let map = resources.selected(81, 2., 0).unwrap().unwrap();
    assert_eq!(map.extent, [2, 2]);
    assert_eq!(map.values, [1.; 4]);
    assert!(resources.selected(40, f32::NAN, 0).unwrap().is_none());
    assert!(resources.selected(80, 2., 0).is_err());
    for aperture in [0., -1., f32::NAN, f32::INFINITY] {
        assert!(resources.selected(81, aperture, 0).is_err());
    }
    std::fs::remove_file(directory.0.join("C81_1")).unwrap();
    assert_eq!(
        resources.selected(81, 2., 0).unwrap().unwrap().values,
        map.values
    );
    assert!(ResourceDirectory::new(&directory.0)
        .unwrap()
        .selected(81, 2., 0)
        .is_err());
}

#[test]
fn every_present_map_is_validated_even_outside_the_requested_camera() {
    let directory = Directory::new();
    for index in 1..=6 {
        write(&directory.0.join(format!("C81_{index}")), [2, 2], &[1.; 4]);
    }
    std::fs::write(directory.0.join("C80_1"), [0; 3]).unwrap();
    assert!(ResourceDirectory::new(&directory.0).is_err());
    std::fs::remove_file(directory.0.join("C80_1")).unwrap();
    write(&directory.0.join("C81_6"), [2, 4], &[1.; 8]);
    assert!(ResourceDirectory::new(&directory.0).is_err());
}

#[test]
fn map_trailer_and_all_payload_values_are_validated() {
    let directory = Directory::new();
    let path = directory.0.join("map");
    for (extent, values) in [
        ([2, 2], vec![1.; 3]),
        ([1, 4], vec![1.; 4]),
        ([0, 0], vec![]),
        ([2, 2], vec![f32::NAN; 4]),
    ] {
        write(&path, extent, &values);
        assert!(load(&path).is_err());
    }
    std::fs::write(&path, vec![0; MAXIMUM_MAP_SAMPLES * 4 + 5]).unwrap();
    assert!(load(&path).is_err());
    assert!(integer(1.5, "test").is_err());
    assert!(integer(-1., "test").is_err());
    assert!(integer(65536., "test").is_err());
}

#[test]
fn aperture_branches_interpolate_and_extrapolate_without_clamping() {
    let maps = (1..=6)
        .map(|value| GainMap {
            extent: [2, 2],
            values: vec![value as f32; 4],
        })
        .collect();
    let resources = ResourceDirectory {
        maps: BTreeMap::from([(81, maps)]),
    };
    for (aperture, focus, expected) in [
        (2., 0, 1.),
        (2., 31, 4.),
        (4., 0, 2.),
        (4., 31, 5.),
        (8., 0, 3.),
        (16., 0, 3.5),
    ] {
        assert_eq!(
            resources
                .selected(81, aperture, focus)
                .unwrap()
                .unwrap()
                .values,
            [expected; 4]
        );
    }
}

#[test]
#[ignore = "requires QUATTRO_BDATA_REFERENCE with native blended grids and manifest.json"]
fn supplied_maps_and_blends_match_all_reference_words() {
    let directory = PathBuf::from(std::env::var_os("QUATTRO_BDATA_REFERENCE").unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    let resources =
        ResourceDirectory::new(Path::new(manifest["resources"].as_str().unwrap())).unwrap();
    assert_eq!(
        resources
            .maps
            .values()
            .map(|maps| maps.iter().map(|map| map.values.len()).sum::<usize>())
            .sum::<usize>(),
        512256
    );
    for fixture in manifest["fixtures"].as_array().unwrap() {
        let map = resources
            .selected(
                fixture["camera"].as_u64().unwrap() as u16,
                fixture["aperture"].as_f64().unwrap() as f32,
                fixture["focus"].as_u64().unwrap() as u16,
            )
            .unwrap()
            .unwrap();
        let expected = std::fs::read(directory.join(fixture["file"].as_str().unwrap())).unwrap();
        assert_eq!(map.extent, [184, 116]);
        assert_eq!(map.values.len() * 4, expected.len());
        for (index, (actual, expected)) in map
            .values
            .iter()
            .zip(expected.as_chunks::<4>().0.iter())
            .enumerate()
        {
            assert_eq!(
                actual.to_bits(),
                u32::from_le_bytes(*expected),
                "fixture {fixture} word {index}"
            );
        }
    }
}
