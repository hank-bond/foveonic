//! Checks sparse correction preparation bounds before GPU submission.

use super::*;

#[test]
fn preparation_rejects_bad_sparse_geometry_and_tables() {
    let g = Gain {
        ordinary: [1., 0., 1., 0.],
        mono: [1., 1., 0., 0.],
        red_green: [1., 0., 1., 0.],
        blue: [1., 0., 0., 0.],
        phases: [1.; 4],
    };
    assert!(Prepared::new([32, 12], [0, 0, 16, 4], vec![0; 32768], vec![g; 6], 1.).is_ok());
    assert!(Prepared::new([32, 12], [0, 0, 0, 4], vec![0; 32768], vec![g; 6], 1.).is_err());
    assert!(Prepared::new([32, 12], [0, 0, 16, 4], vec![0; 32767], vec![g; 6], 1.).is_err());
    assert!(Prepared::new([32, 12], [0, 0, 16, 4], vec![0; 32768], vec![g; 5], 1.).is_err());
}
