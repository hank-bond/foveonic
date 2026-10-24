//! Checks recursive discovery, folder boundaries, and navigation through virtualized groups.

use super::*;

#[test]
fn recursive_open_keeps_root_images_first_and_ignores_metadata_and_link_loops() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for name in [
        "z.x3f",
        "a.X3F",
        "DCIM/100SIGMA/z.x3f",
        "DCIM/100SIGMA/a.x3f",
        "DCIM/101SIGMA/a.x3f",
        ".Spotlight-V100/ignored.x3f",
        "._resource.x3f",
        "other.jpg",
    ] {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, []).unwrap();
    }
    std::fs::create_dir(root.join("empty")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(root, root.join("DCIM/loop")).unwrap();
    let expected = [
        "a.X3F",
        "z.x3f",
        "DCIM/100SIGMA/a.x3f",
        "DCIM/100SIGMA/z.x3f",
        "DCIM/101SIGMA/a.x3f",
    ]
    .map(|name| root.join(name));
    let mut browser = crate::browser::Browser::new(egui::Context::default());
    for source in [root.to_owned(), root.join("z.x3f")] {
        browser.open(&source).unwrap();
        assert_eq!(browser.files, expected);
        assert_eq!(browser.root.as_deref(), Some(root));
    }
    assert!(browser.open(&root.join("missing")).is_err());
    assert_eq!(browser.files, expected);
    assert_eq!(browser.root.as_deref(), Some(root));
}

#[test]
fn partial_grid_rows_end_at_folder_boundaries_and_arrows_skip_the_headings() {
    let root = Path::new("card");
    let files = [
        "a.x3f",
        "DCIM/100SIGMA/a.x3f",
        "DCIM/100SIGMA/b.x3f",
        "DCIM/100SIGMA/c.x3f",
        "DCIM/101SIGMA/a.x3f",
        "DCIM/101SIGMA/b.x3f",
    ]
    .map(|name| root.join(name));
    let layout = Layout::new(&files, Some(root), 2, 146., 24., 6.);
    let photo_ranges = layout
        .rows
        .iter()
        .filter_map(|row| match &row.content {
            Content::Photos(range) => Some(range.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(photo_ranges, [0..1, 1..3, 3..4, 4..6]);
    assert_eq!(layout.rows.len(), 6);
    for (current, key, expected) in [
        (0, egui::Key::ArrowDown, 1),
        (2, egui::Key::ArrowDown, 3),
        (3, egui::Key::ArrowDown, 4),
        (5, egui::Key::ArrowUp, 3),
        (3, egui::Key::ArrowRight, 4),
        (4, egui::Key::ArrowLeft, 3),
    ] {
        assert_eq!(layout.arrow(Some(current), key), Some(expected));
    }
    let selected = layout.photo_row(4);
    let visible = layout.visible(selected.top, selected.top + selected.height);
    assert_eq!(visible.len(), 1);
    assert!(matches!(&visible[0].content, Content::Photos(range) if range == &(4..6)));
    let filtered = Layout::new(&files[..4], Some(root), 1, 40., 24., 6.);
    let headings = filtered
        .rows
        .iter()
        .filter_map(|row| match &row.content {
            Content::Folder(path) => Some(path),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        headings,
        [root.join("DCIM/100SIGMA")].iter().collect::<Vec<_>>()
    );
}
