// Encoded brightness cells cover complete 2×2 image blocks, including odd edges.
fn brightness_cell_index(point: vec2<u32>, extent: vec2<u32>) -> u32 {
    let cell = point / 2u;
    return cell.y * ((extent.x + 1u) / 2u) + cell.x;
}
