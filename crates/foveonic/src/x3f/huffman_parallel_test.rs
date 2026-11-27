//! Checks fragment lanes against serial decoding at the sample capacity, damage and empty lanes.

use super::*;

/// "0" is a zero difference; "10x" is ±1; "11" is not a code.
fn table() -> Table {
    Table::new(&[(0, 1), (2048, 2)]).unwrap()
}

fn decoded(split: &Split, input: &[u8], part: usize) -> Option<usize> {
    let table = table();
    let lookup = table.differences();
    split
        .unpack(&table, &lookup, input, 0, part)
        .unwrap()
        .map(|lanes| lanes.iter().map(Vec::len).sum())
}

#[test]
fn a_fragment_shares_one_sample_capacity_across_its_lanes() {
    // Every bit is a symbol, so the first of two fragments holds 16384
    // differences, about 4096 per lane.
    let input = vec![0; 4096];
    let split = Split::plan(&table(), &input, 2, 8000).unwrap();
    assert_eq!(decoded(&split, &input, 0), None);
    let split = Split::plan(&table(), &input, 2, 32768).unwrap();
    assert!(decoded(&split, &input, 0).is_some());
}

#[test]
fn an_invalid_code_inside_a_middle_lane_fails_its_fragment() {
    let mut input = vec![0; 4096];
    // Lane 1 of the first fragment covers bits 4096..8192.
    input[768] = 0xff;
    let split = Split::plan(&table(), &input, 2, 32768).unwrap();
    assert_eq!(decoded(&split, &input, 0), None);
}

#[test]
fn the_join_keeps_only_the_declared_length_without_growing() {
    // 32768 one-bit symbols, so the last fragment runs past the declared
    // length into the stream's padding.
    let input = vec![0; 4096];
    let length = 30000;
    let joined = decode_bytes(&table(), &input, length, 7, 2)
        .unwrap()
        .unwrap();
    assert_eq!(joined.capacity(), length);
    assert_eq!(joined, table().decode_bytes(&input, length, 7).unwrap());
}

#[test]
fn a_short_stream_reuses_boundaries_as_empty_lanes() {
    let input = [0x92, 0x49, 0x24, 0x92];
    let split = Split::plan(&table(), &input, 3, 64).unwrap();
    assert!(split.boundaries.windows(2).any(|pair| pair[0] == pair[1]));
    let serial = table().decode_bytes(&input, 10, 7).unwrap();
    assert_eq!(
        decode_bytes(&table(), &input, 10, 7, 3).unwrap(),
        Some(serial)
    );
}
