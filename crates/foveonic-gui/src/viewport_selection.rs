//! Keeps bounded viewport editing targets separate from browser selection and active-photo context.

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    Replace,
    Toggle,
    Range,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub path: PathBuf,
    pub gesture: Gesture,
}

#[derive(Default, Debug)]
pub struct Selection {
    browser: Vec<PathBuf>,
    targets: Vec<PathBuf>,
    anchor: Option<PathBuf>,
}
impl Selection {
    pub fn paths(&self) -> &[PathBuf] {
        &self.targets
    }
    pub fn contains(&self, path: &Path) -> bool {
        self.targets.iter().any(|target| target == path)
    }
    /// Browser membership and order initialize the subset. Active-photo changes
    /// do not participate, so viewing another target preserves explicit selection.
    pub fn sync_browser(&mut self, paths: &[PathBuf], limit: usize) -> bool {
        let limit = limit.max(1);
        if self.browser != paths {
            self.browser = paths.to_vec();
            let next: Vec<_> = paths.iter().take(limit).cloned().collect();
            let changed = self.targets != next;
            self.targets = next;
            self.anchor = self.targets.first().cloned();
            return changed;
        }
        let changed = self.targets.len() > limit;
        self.targets.truncate(limit);
        changed
    }
    pub fn apply(&mut self, request: Request, limit: usize) -> bool {
        self.apply_order(request, limit, None)
    }
    pub fn apply_displayed(
        &mut self,
        request: Request,
        limit: usize,
        displayed: &[PathBuf],
    ) -> bool {
        self.apply_order(request, limit, Some(displayed))
    }
    fn apply_order(
        &mut self,
        request: Request,
        limit: usize,
        displayed: Option<&[PathBuf]>,
    ) -> bool {
        let limit = limit.max(1);
        if !self.browser.contains(&request.path) {
            return false;
        }
        let previous = self.targets.clone();
        self.targets.truncate(limit);
        match request.gesture {
            Gesture::Replace => {
                self.targets = vec![request.path.clone()];
                self.anchor = Some(request.path);
            }
            Gesture::Toggle => {
                if let Some(index) = self.targets.iter().position(|path| path == &request.path) {
                    self.targets.remove(index);
                    self.anchor = Some(request.path);
                } else if self.targets.len() < limit {
                    self.targets.push(request.path.clone());
                    self.anchor = Some(request.path);
                }
            }
            Gesture::Range => {
                let order = displayed.unwrap_or(&self.browser);
                let Some(clicked) = order.iter().position(|path| path == &request.path) else {
                    return false;
                };
                let anchor = self
                    .anchor
                    .as_ref()
                    .and_then(|anchor| order.iter().position(|path| path == anchor))
                    .unwrap_or(clicked);
                // Preserve the anchor and take the nearest range members first.
                // Storage remains in browser order after choosing the visual range.
                let (first, last) = if clicked >= anchor {
                    (anchor, clicked.min(anchor.saturating_add(limit - 1)))
                } else {
                    (clicked.max(anchor.saturating_sub(limit - 1)), anchor)
                };
                self.targets = order[first..=last]
                    .iter()
                    .filter(|path| self.browser.contains(path))
                    .cloned()
                    .collect();
            }
        }
        let browser = &self.browser;
        self.targets
            .sort_by_key(|path| browser.iter().position(|candidate| candidate == path));
        previous != self.targets
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn photos() -> Vec<PathBuf> {
        (0..20)
            .map(|index| PathBuf::from(format!("photo-{index}.x3f")))
            .collect()
    }
    fn request(paths: &[PathBuf], index: usize, gesture: Gesture) -> Request {
        Request {
            path: paths[index].clone(),
            gesture,
        }
    }
    #[test]
    fn browser_order_initializes_bounded_targets_without_limiting_browser_selection() {
        let mut selection = Selection::default();
        let mut paths = photos();
        assert!(selection.sync_browser(&paths, 8));
        assert_eq!(selection.paths(), &paths[..8]);
        selection.apply(request(&paths, 12, Gesture::Replace), 8);
        assert!(!selection.sync_browser(&paths, 8));
        assert_eq!(selection.paths(), &paths[12..13]);
        paths.reverse();
        assert!(selection.sync_browser(&paths, 8));
        assert_eq!(selection.paths(), &paths[..8]);
        assert_eq!(paths.len(), 20);
    }
    #[test]
    fn toggles_are_bounded_ordered_and_can_leave_no_edit_targets() {
        let paths = photos();
        let mut selection = Selection::default();
        selection.sync_browser(&paths, 2);
        assert!(!selection.apply(request(&paths, 4, Gesture::Toggle), 2));
        selection.apply(request(&paths, 0, Gesture::Toggle), 2);
        selection.apply(request(&paths, 4, Gesture::Toggle), 2);
        assert_eq!(selection.paths(), &[paths[1].clone(), paths[4].clone()]);
        selection.apply(request(&paths, 1, Gesture::Toggle), 2);
        selection.apply(request(&paths, 4, Gesture::Toggle), 2);
        assert!(selection.paths().is_empty());
        assert!(!selection.sync_browser(&paths, 2));
        assert!(selection.paths().is_empty());
    }
    #[test]
    fn shift_range_keeps_anchor_and_caps_in_both_directions() {
        let paths = photos();
        let mut selection = Selection::default();
        selection.sync_browser(&paths, 3);
        selection.apply(request(&paths, 8, Gesture::Replace), 3);
        selection.apply(request(&paths, 15, Gesture::Range), 3);
        assert_eq!(selection.paths(), &paths[8..11]);
        selection.apply(request(&paths, 0, Gesture::Range), 3);
        assert_eq!(selection.paths(), &paths[6..9]);
        selection.apply(request(&paths, 7, Gesture::Range), 3);
        assert_eq!(selection.paths(), &paths[7..9]);
    }
    #[test]
    fn rejected_toggle_does_not_move_the_range_anchor() {
        let paths = photos();
        let mut selection = Selection::default();
        selection.sync_browser(&paths, 3);
        assert!(!selection.apply(request(&paths, 12, Gesture::Toggle), 3));
        selection.apply(request(&paths, 8, Gesture::Range), 3);
        assert_eq!(selection.paths(), &paths[..3]);
    }

    #[test]
    fn limit_changes_preserve_explicit_subset_and_unknown_photos_do_nothing() {
        let paths = photos();
        let mut selection = Selection::default();
        selection.sync_browser(&paths, 8);
        selection.sync_browser(&paths, 3);
        assert_eq!(selection.paths(), &paths[..3]);
        selection.sync_browser(&paths, 12);
        assert_eq!(selection.paths(), &paths[..3]);
        assert!(!selection.apply(
            Request {
                path: "unknown.x3f".into(),
                gesture: Gesture::Replace
            },
            12
        ));
        assert_eq!(selection.paths(), &paths[..3]);
        selection.sync_browser(&[], 8);
        assert!(selection.paths().is_empty());
    }
    #[test]
    fn displayed_ranges_follow_visual_order_but_store_browser_order() {
        let paths = photos();
        let displayed = vec![
            paths[4].clone(),
            paths[1].clone(),
            paths[7].clone(),
            paths[2].clone(),
        ];
        let mut selection = Selection::default();
        selection.sync_browser(&paths, 8);
        selection.apply(request(&paths, 4, Gesture::Replace), 8);
        selection.apply_displayed(request(&paths, 2, Gesture::Range), 3, &displayed);
        assert_eq!(
            selection.paths(),
            &[paths[1].clone(), paths[4].clone(), paths[7].clone()]
        );
        selection.apply_displayed(request(&paths, 2, Gesture::Replace), 8, &displayed);
        selection.apply_displayed(request(&paths, 4, Gesture::Range), 2, &displayed);
        assert_eq!(selection.paths(), &[paths[2].clone(), paths[7].clone()]);
        // An anchor hidden by overflow has no position in the displayed range.
        selection.apply(request(&paths, 15, Gesture::Replace), 8);
        selection.apply_displayed(request(&paths, 7, Gesture::Range), 8, &displayed);
        assert_eq!(selection.paths(), &paths[7..8]);
    }
}
