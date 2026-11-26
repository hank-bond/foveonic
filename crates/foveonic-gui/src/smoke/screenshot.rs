//! Loads the fixed README screenshot recipe and defines its capture steps.

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenshotRecipe {
    pub source: String,
    pub window_size: [f32; 2],
    pub organizer_width: f32,
    pub tools_width: f32,
    pub(super) zoom: f32,
    pub(super) pan: [f32; 2],
    pub(super) settings: foveonic::render::Settings,
}

pub fn screenshot_active() -> bool {
    std::env::var_os("FOVEONIC_GUI_SMOKE_DIR").is_some()
        && std::env::var_os("FOVEONIC_GUI_SMOKE_SCREENSHOT").is_some()
}

pub fn screenshot_recipe() -> ScreenshotRecipe {
    serde_json::from_str(include_str!("../../assets/gui-screenshot.json"))
        .expect("Invalid screenshot recipe")
}

pub(super) const TARGETS: &[&str] = &["preview", "readme"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readme_recipe_has_fixed_valid_source_view_and_photographic_settings() {
        let recipe = screenshot_recipe();
        assert_eq!(recipe.source, "yellow-flowers-and-stroller.x3f");
        assert_eq!(recipe.window_size, [1325., 1097.]);
        assert_eq!(recipe.organizer_width, 264.);
        assert_eq!(recipe.tools_width, 330.);
        assert!(recipe.zoom.is_finite() && recipe.zoom >= 1.);
        assert!(recipe.pan.iter().all(|value| (0. ..=1.).contains(value)));
        recipe.settings.validate_values().unwrap();
        assert_eq!(recipe.settings.white_balance, "Sunlight");
        assert_eq!(recipe.settings.color_mode, "Standard");
        assert!(recipe.settings.reconstruction.is_some());
        assert_eq!(recipe.settings.highlight_threshold_ev, Some(0.71));
        assert_eq!(recipe.settings.tone.highlights, 0.);
        assert_eq!(TARGETS, ["preview", "readme"]);
    }
}
