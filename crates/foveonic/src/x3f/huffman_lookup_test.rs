//! Checks fused Huffman differences against category decoding, split planes, and exact original-file loading.

use super::*;
use std::{path::PathBuf, time::Instant};

fn category_plane(
    table: &Table,
    input: &[u8],
    width: usize,
    height: usize,
    seed: u16,
    maximum: u16,
) -> Result<Vec<u16>> {
    let mut output = Vec::with_capacity(width * height);
    let mut bits = Bits::new(input);
    let mut vertical = [[seed; 2]; 2];
    let mut horizontal = [seed; 2];
    for y in 0..height {
        crate::priority::checkpoint_or_cancel()?;
        for x in 0..width {
            let predictor = if x < 2 {
                if y < 2 {
                    seed
                } else {
                    vertical[y & 1][x & 1]
                }
            } else {
                horizontal[x & 1]
            };
            let value = (predictor as i16).wrapping_add(table.difference(&mut bits)?) as u16;
            if value > maximum {
                return Err(damaged("decoded sample outside declared range"));
            }
            horizontal[x & 1] = value;
            if x < 2 {
                vertical[y & 1][x & 1] = value;
            }
            output.push(value);
        }
    }
    Ok(output)
}

fn assert_same<T: std::fmt::Debug + PartialEq>(expected: Result<T>, actual: Result<T>) {
    assert_eq!(expected.as_ref().ok(), actual.as_ref().ok());
    assert_eq!(
        expected.as_ref().err().map(ToString::to_string),
        actual.as_ref().err().map(ToString::to_string)
    );
}

#[test]
fn fused_differences_preserve_payloads_and_every_short_bit_boundary() {
    let mut long = vec![0; 18];
    long[0] = 1;
    long[17] = 0x8001;
    for table in [
        Table::new(&[(0, 1)]).unwrap(),
        Table::new(&[(0, 1), (2048, 2), (3072, 2)]).unwrap(),
        Table::new(&(0..13).map(|i| (i << 8, 4)).collect::<Vec<_>>()).unwrap(),
        Table::packed(&long).unwrap(),
    ] {
        let lookup = table.differences();
        for prefix in 0..4096u16 {
            let input = [((prefix << 4) >> 8) as u8, (prefix << 4) as u8, 0x95];
            for available in 0..=24 {
                let stream = || {
                    let mut bits = Bits::new(&input);
                    bits.peek();
                    bits.buffered_bits = available;
                    bits
                };
                let mut category = stream();
                let mut fused = stream();
                let expected = table.difference(&mut category);
                let actual = table.looked_up_difference(&lookup, &mut fused);
                if expected.is_ok() {
                    assert_eq!(category.buffered_bits, fused.buffered_bits);
                }
                assert_same(expected, actual);
            }
        }
    }
}

#[test]
fn fused_planes_preserve_predictors_damage_and_geometry_limits() {
    let table = Table::new(&[(0, 1), (2048, 2), (3072, 2)]).unwrap();
    let input: Vec<_> = (0..83).map(|i| (i * 73 + 19) as u8).collect();
    for width in [1, 2, 3, 7, 8, 17] {
        for height in [1, 2, 3, 9] {
            for length in [0, 1, 2, 7, input.len()] {
                for seed in [0, 512, 4095] {
                    assert_same(
                        category_plane(&table, &input[..length], width, height, seed, 4095),
                        table.decode(&input[..length], width, height, seed, 4095),
                    );
                }
            }
        }
    }
    for (width, height, seed) in [
        (0, 1, 0),
        (1, 0, 0),
        (1, 1, 4096),
        (65536, 1, 0),
        (8192, 8192, 0),
    ] {
        assert!(table.decode(&[], width, height, seed, 4095).is_err());
    }
    let gate = std::sync::Arc::new(crate::priority::Gate::default());
    crate::priority::interruptible(gate.clone(), || {
        gate.interrupt_speculative();
        let error = table.decode(&[0], 4, 2, 512, 4095).unwrap_err();
        assert!(error.is::<crate::priority::Interrupted>());
    });
}

#[test]
fn split_plane_reconstructs_prediction_across_unaligned_fragments() {
    let table = Table::new(&[(0, 1), (2048, 2)]).unwrap();
    for width in [1, 2, 13, 32] {
        let height = 257;
        let mut input = Vec::new();
        let mut bits = 0;
        for i in 0..width * height {
            let code = match i % 17 {
                0..=5 => "100",
                10..=15 => "101",
                _ => "0",
            };
            for b in code.bytes() {
                if bits % 8 == 0 {
                    input.push(0);
                }
                *input.last_mut().unwrap() |= (b - b'0') << (7 - bits % 8);
                bits += 1;
            }
        }
        let expected = table.decode(&input, width, height, 512, 4095).unwrap();
        for fragments in [2, 3, 8] {
            // Require the split path to succeed without silently using scalar decoding.
            let plane = Plane {
                input: &input,
                width,
                height,
                seed: 512,
            };
            let actual = reconstruct_split(&table, plane, fragments).unwrap();
            assert_eq!(actual, expected);
        }
    }
}

fn reconstruct_split(table: &Table, plane: Plane<'_>, fragments: usize) -> Option<Vec<u16>> {
    let lookup = table.differences();
    let split = parallel::Split::new(table, plane, fragments)?;
    let parts = (0..split.len())
        .map(|part| split.unpack(table, &lookup, plane.input, 0, part).unwrap())
        .collect();
    parallel::reconstruct(parts, plane, 4095).unwrap()
}

#[test]
fn split_plane_preserves_the_declared_sample_end_and_first_damage() {
    let table = Table::new(&[(0, 1), (2048, 2)]).unwrap();
    let mut input = [0; 10];
    input[9] = 0xff;
    assert_eq!(
        reconstruct_split(
            &table,
            Plane {
                input: &input,
                width: 5,
                height: 13,
                seed: 512
            },
            3
        ),
        Some(vec![512; 65]),
    );
    for input in [
        &input[..8],
        &input[..],
        &[0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0xff],
    ] {
        let plane = Plane {
            input,
            width: 5,
            height: 15,
            seed: 0,
        };
        assert_same(
            table.decode(input, 5, 15, 0, 4095),
            table
                .decode_split_planes([plane; 3], 4095, [3; 3], 30)
                .map(|mut planes| planes.remove(0).0),
        );
    }

    let gate = std::sync::Arc::new(crate::priority::Gate::default());
    crate::priority::interruptible(gate.clone(), || {
        gate.interrupt_speculative();
        let plane = Plane {
            input: &input,
            width: 5,
            height: 13,
            seed: 512,
        };
        let error = table
            .decode_split_planes([plane; 3], 4095, [3; 3], 30)
            .unwrap_err();
        assert!(error.is::<crate::priority::Interrupted>());
    });
}

#[test]
#[ignore = "requires DECODER_RESEARCH_FILES, optional DECODER_RESEARCH_REPEATS, serial test execution"]
fn profile_original_loading() {
    let paths: Vec<PathBuf> =
        std::env::split_paths(&std::env::var_os("DECODER_RESEARCH_FILES").unwrap()).collect();
    let repeats: usize = std::env::var("DECODER_RESEARCH_REPEATS")
        .ok()
        .map_or(5, |v| v.parse().unwrap());
    for path in paths {
        let expected = crate::source::Source::open(&path).unwrap();
        for repeat in 0..repeats {
            let began = Instant::now();
            let source = crate::source::Source::open(&path).unwrap();
            let milliseconds = began.elapsed().as_secs_f64() * 1000.;
            assert_eq!(source.samples(), expected.samples());
            println!(
                "{}",
                crate::profile::report(
                    "decoder_original_loading",
                    crate::profile::Mode::Off,
                    serde_json::json!({"source":path,"repeat":repeat,"open_ms":milliseconds,"stages":source.load_timings(),"exact":true})
                )
            );
        }
    }
}
