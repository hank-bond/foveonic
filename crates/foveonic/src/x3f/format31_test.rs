use super::*;

#[test]
fn rejects_unestablished_formats_and_geometry() {
    for format in [31, 32, 34, 36, 38, 40, 41, 42] {
        assert!(decode(&[], 5888, 3672, format).is_err(), "format {format}");
    }
    for (width, height, format) in [
        (5888, 3672, 34),
        (5889, 3672, 35),
        (5888, 3673, 35),
        (4000, 2000, 35),
        (0, 0, 37),
    ] {
        assert!(decode(&[], width, height, format).is_err());
    }
}

#[test]
#[ignore = "requires QUATTRO_X3F and QUATTRO_NATIVE_SENSOR"]
fn packed_sensor_matches_every_reference_word() {
    let data = std::fs::read(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let bytes = Bytes(&data);
    let directory = bytes.offset(data.len() - 4).unwrap();
    let count = bytes.offset(directory + 8).unwrap();
    let section = (0..count)
        .find_map(|i| {
            let entry = directory + 12 + i * 12;
            let section = bytes
                .range(
                    bytes.offset(entry).unwrap(),
                    bytes.offset(entry + 4).unwrap(),
                )
                .unwrap();
            (section.starts_with(b"SECi")
                && matches!(Bytes(section).u32(12).unwrap(), 35 | 37 | 39))
            .then_some(section)
        })
        .unwrap();
    let header = Bytes(section);
    let output = decode(
        section,
        header.offset(16).unwrap(),
        header.offset(20).unwrap(),
        header.u32(12).unwrap(),
    )
    .unwrap();
    let reference = std::fs::read(std::env::var_os("QUATTRO_NATIVE_SENSOR").unwrap()).unwrap();
    assert!(reference.len() >= output.len() * 4);
    for (index, (actual, expected)) in output
        .iter()
        .zip(reference.as_chunks::<4>().0.iter())
        .enumerate()
    {
        assert_eq!(
            actual.to_bits(),
            u32::from_le_bytes(*expected),
            "sample {index}"
        );
    }
}
