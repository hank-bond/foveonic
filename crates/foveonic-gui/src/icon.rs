//! Loads the window icon while leaving macOS Dock appearances to the app bundle.

const PNG: &[u8] = include_bytes!("../assets/icon.png");

pub fn window_icon() -> eframe::egui::IconData {
    let image = image::load_from_memory(PNG)
        .expect("embedded application icon")
        .into_rgba8();
    eframe::egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn icon_has_a_light_background_transparent_exterior_and_preserved_artwork() {
        let icon = super::window_icon();
        assert_eq!([icon.width, icon.height], [1024, 1024]);
        assert_eq!(icon.rgba[3], 0);
        // Apple's compiler supplies these 100-pixel margins around the tile.
        // Guard that placement so a full-canvas preview cannot make the Dock icon oversized.
        let alpha = |x: usize, y: usize| icon.rgba[(y * 1024 + x) * 4 + 3];
        assert!(alpha(99, 512) < 128);
        assert!(alpha(100, 512) >= 128);
        assert!(alpha(923, 512) >= 128);
        assert!(alpha(924, 512) < 128);
        let background = &icon.rgba[(150 * 1024 + 512) * 4..(150 * 1024 + 512) * 4 + 4];
        assert!(background[..3].iter().all(|v| *v > 225));
        assert_eq!(background[3], 255);
        let bottom = &icon.rgba[(870 * 1024 + 512) * 4..(870 * 1024 + 512) * 4 + 4];
        assert_eq!(bottom[3], 255);
        assert!(background[0] > bottom[0] + 25);
        assert!(icon
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[3] == 255 && p[..3].iter().all(|v| *v < 80)));
        // Icon Composer dithers its sRGB raster export by about one code value.
        for expected in [[39_u8, 103, 219], [115, 200, 66]] {
            assert!(icon.rgba.as_chunks::<4>().0.iter().any(|p| {
                p[3] == 255
                    && p[..3]
                        .iter()
                        .zip(expected)
                        .all(|(&a, b)| a.abs_diff(b) <= 2)
            }));
        }
        assert!(icon
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0] > 220 && p[1] < 90 && p[2] < 80 && p[3] == 255));
    }
}
