use super::*;

#[test]
fn rejects_unestablished_formats_and_geometry() {
    for format in [32, 34, 36, 38, 40, 42] {
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
    .unwrap()
    .pixels;
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

#[test]
#[ignore = "requires QUATTRO_X3F"]
fn packed_sensor_matches_serial_planes() {
    let path = std::env::var_os("QUATTRO_X3F").unwrap();
    let data = std::fs::read(&path).unwrap();
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
    let bytes = Bytes(section);
    let width = bytes.offset(16).unwrap();
    let height = bytes.offset(20).unwrap();
    let format = bytes.u32(12).unwrap();
    let blue_width = usize::from(bytes.u16(36).unwrap());
    let words = (0..18)
        .map(|i| bytes.u16(48 + i * 2).unwrap())
        .collect::<Vec<_>>();
    let table = Table::packed(&words).unwrap();
    let serial_start = std::time::Instant::now();
    let mut offset = 96;
    let planes = (0..3)
        .map(|channel| {
            let length = bytes.offset(84 + channel * 4).unwrap();
            let plane = table
                .decode_bounded(
                    bytes.range(offset, length).unwrap(),
                    usize::from(bytes.u16(28 + channel * 4).unwrap()),
                    usize::from(bytes.u16(30 + channel * 4).unwrap()),
                    bytes.u16(40 + channel * 2).unwrap(),
                    16383,
                )
                .unwrap();
            offset += (length + 15) & !15;
            plane
        })
        .collect::<Vec<_>>();
    let serial_ms = serial_start.elapsed().as_secs_f64() * 1000.;
    let mut timings = Vec::new();
    for _ in 0..3 {
        let start = std::time::Instant::now();
        let output = decode(section, width, height, format).unwrap().pixels;
        timings.push(start.elapsed().as_secs_f64() * 1000.);
        for (index, cell) in output.as_chunks::<6>().0.iter().enumerate() {
            let x = index % (width / 2);
            let y = index / (width / 2);
            let blue_x = if blue_width != width && (y * 2).is_multiple_of(8) && x.is_multiple_of(16)
            {
                width + (x / 16) * 2
            } else {
                x * 2
            };
            let expected = [
                planes[0][index],
                planes[1][index],
                planes[2][y * 2 * blue_width + blue_x],
                planes[2][y * 2 * blue_width + blue_x + 1],
                planes[2][(y * 2 + 1) * blue_width + blue_x],
                planes[2][(y * 2 + 1) * blue_width + blue_x + 1],
            ];
            for (actual, expected) in cell.iter().zip(expected) {
                assert_eq!(
                    actual.to_bits(),
                    f32::from(expected).to_bits(),
                    "cell {index}"
                );
            }
        }
    }
    let start = std::time::Instant::now();
    let source = crate::source::Source::open(&path).unwrap();
    eprintln!("decode benchmark {:?}: serial planes {serial_ms:.3} ms, packed {timings:?} ms, source {:.3} ms {:?}", path, start.elapsed().as_secs_f64() * 1000., source.load_timings());
}

#[test]
fn independent_planes_preserve_order_and_report_bounded_errors() {
    let mut section = vec![0u8; 144];
    for (index, value) in [1u16, 1, 1, 1, 2, 2].into_iter().enumerate() {
        section[28 + index * 2..30 + index * 2].copy_from_slice(&value.to_le_bytes());
    }
    for (channel, seed) in [10u16, 20, 30].into_iter().enumerate() {
        section[40 + channel * 2..42 + channel * 2].copy_from_slice(&seed.to_le_bytes());
        section[84 + channel * 4..88 + channel * 4].copy_from_slice(&2u32.to_le_bytes());
    }
    section[48..50].copy_from_slice(&1u16.to_le_bytes());
    assert_eq!(
        decode(&section, 2, 2, 37).unwrap().pixels,
        [10., 20., 30., 30., 30., 30.]
    );
    assert!(decode(&section[..129], 2, 2, 37).is_err());
    section[128] = 0xff;
    assert!(decode(&section, 2, 2, 37).is_err());
    section[128] = 0;
    section[44..46].copy_from_slice(&16384u16.to_le_bytes());
    assert!(decode(&section, 2, 2, 37).is_err());
}

// Independently encode zero and unit differences, including the parity predictors.
fn unsigned_fixture(width: usize, height: usize, packed: bool, extra: usize) -> Vec<u8> {
    let low = if packed { 2 } else { 1 };
    let shapes = [
        (width / low, height / low),
        (width / low, height / low),
        (width + extra, height),
    ];
    let mut section = vec![0; 96];
    section[48..50].copy_from_slice(&1u16.to_le_bytes());
    section[50..52].copy_from_slice(&0x8001u16.to_le_bytes());
    for (channel, (w, h)) in shapes.into_iter().enumerate() {
        section[28 + channel * 4..30 + channel * 4].copy_from_slice(&(w as u16).to_le_bytes());
        section[30 + channel * 4..32 + channel * 4].copy_from_slice(&(h as u16).to_le_bytes());
        let seed = (channel as u16 + 1) * 10;
        section[40 + channel * 2..42 + channel * 2].copy_from_slice(&seed.to_le_bytes());
        let mut plane = vec![i32::from(seed); w * h];
        if channel == 2 && extra != 0 {
            plane[width] += 1;
            plane[width + 1] -= 1;
        }
        let mut bits = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let predicted = if x >= 2 {
                    plane[y * w + x - 2]
                } else if y >= 2 {
                    plane[(y - 2) * w + x]
                } else {
                    i32::from(seed)
                };
                let difference = plane[y * w + x] - predicted;
                match difference {
                    0 => bits.push(false),
                    1 => bits.extend([true, true]),
                    -1 => bits.extend([true, false]),
                    _ => panic!("fixture needs a larger difference category"),
                }
            }
        }
        let mut stream = vec![0u8; bits.len().div_ceil(8) + 2];
        for (i, value) in bits.into_iter().enumerate() {
            stream[i / 8] |= u8::from(value) << (7 - i % 8);
        }
        section[84 + channel * 4..88 + channel * 4]
            .copy_from_slice(&(stream.len() as u32).to_le_bytes());
        stream.resize(stream.len().div_ceil(16) * 16, 0);
        section.extend(stream);
    }
    section
}

#[test]
fn unsigned_equal_resolution_planes_keep_rgb_and_reduced_special_columns() {
    for format in [31, 35, 37, 39, 41] {
        let extra = if format == 35 { 2 } else { 0 };
        let section = unsigned_fixture(32, 8, false, extra);
        let decoded = decode(&section, 32, 8, format).unwrap();
        assert_eq!(
            decoded.storage,
            crate::source::SourceStorage::InterleavedRgb
        );
        assert_eq!(decoded.pixels.len(), 32 * 8 * 3);
        for (i, rgb) in decoded.pixels.as_chunks::<3>().0.iter().enumerate() {
            let blue = if extra != 0 && i == 0 {
                31.
            } else if extra != 0 && i == 16 {
                29.
            } else {
                30.
            };
            assert_eq!(*rgb, [10., 20., blue], "format {format}, pixel {i}");
        }
        if extra != 0 {
            assert!(decode(&section, 32, 8, 37).is_err());
        }
    }
}

#[test]
fn packed_special_columns_still_replace_physical_pairs() {
    let section = unsigned_fixture(64, 8, true, 4);
    let decoded = decode(&section, 64, 8, 35).unwrap();
    assert_eq!(
        decoded.storage,
        crate::source::SourceStorage::PackedRgbBluePhases
    );
    assert_eq!(decoded.pixels.len(), 32 * 4 * 6);
    assert_eq!(&decoded.pixels[..6], &[10., 20., 31., 29., 30., 30.]);
    assert!(decoded.pixels[6..]
        .as_chunks::<6>()
        .0
        .iter()
        .all(|v| *v == [10., 20., 30., 30., 30., 30.]));
}
