//! Orders a bounded selection neighborhood for prefetch and cache retention.

use std::path::{Path, PathBuf};

/// Distances follow browser order. Forward neighbors win ties, including across folders.
pub(super) fn around(
    files: &[PathBuf],
    selected: Option<&Path>,
    behind: usize,
    ahead: usize,
) -> Vec<PathBuf> {
    let Some(index) = selected.and_then(|path| files.iter().position(|file| file == path)) else {
        return Vec::new();
    };
    let behind = behind.min(index);
    let ahead = ahead.min(files.len() - index - 1);
    let mut paths = Vec::with_capacity(1 + behind + ahead);
    paths.push(files[index].clone());
    for distance in 1..=behind.max(ahead) {
        if distance <= ahead {
            paths.push(files[index + distance].clone());
        }
        if distance <= behind {
            paths.push(files[index - distance].clone());
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    fn photos() -> Vec<PathBuf> {
        (0..30).map(|i| PathBuf::from(i.to_string())).collect()
    }
    #[test]
    fn defaults_bound_the_window_and_favor_near_forward_neighbors() {
        let photos = photos();
        let expected = [10, 11, 9, 12, 8, 13, 7, 14, 15, 16, 17].map(|i| photos[i].clone());
        assert_eq!(around(&photos, Some(&photos[10]), 3, 7), expected);
        assert_eq!(around(&photos, Some(&photos[0]), 3, 7), photos[..8]);
        assert_eq!(
            around(&photos, Some(&photos[29]), 3, 7),
            [29, 28, 27, 26].map(|i| photos[i].clone())
        );
    }
    #[test]
    fn zero_extreme_and_missing_selection_counts_stay_bounded() {
        let photos = photos();
        assert_eq!(
            around(&photos, Some(&photos[10]), 0, 0),
            [photos[10].clone()]
        );
        assert_eq!(
            around(&photos, Some(&photos[10]), usize::MAX, usize::MAX).len(),
            photos.len()
        );
        assert!(around(&photos, None, 3, 7).is_empty());
        assert!(around(&photos, Some(Path::new("missing")), 3, 7).is_empty());
    }
}
