//! Explains editing controls, inspection stages, scopes, and navigator gestures in photographic language.

use crate::preferences::Scope;
use eframe::egui::{self, os::OperatingSystem};

/// Attach the same explanation to editable and read-only controls.
pub fn attach(response: egui::Response, text: &str) -> egui::Response {
    response.on_hover_text(text).on_disabled_hover_text(text)
}

pub fn control(response: egui::Response, label: &str) -> egui::Response {
    match editing(label) {
        Some(text) => attach(response, text),
        None => response,
    }
}

pub fn editing(label: &str) -> Option<&'static str> {
    Some(match label {
        "White balance" => {
            "Corrects the color cast of the light so whites and grays look neutral. Choose a preset close to the lighting, then judge skin tones and neutral objects. It also affects how color noise is treated."
        }
        "Color mode" => {
            "Chooses the camera's color and contrast look, such as a more natural or vivid rendering. This is a starting look, not just a saturation adjustment; it can also change the tone curve."
        }
        "Sensor gain" => {
            "An ISO-like adjustment applied early in processing. It turns the recorded signal up or down, including its noise, before noise reduction and color processing. 1× leaves it unchanged; 2× doubles it, equivalent to one stop. Raising it can make noise more visible and push highlights toward white. It does not change the ISO used to take the photo. Start with Exposure for ordinary brightness changes."
        }
        "Exposure" => {
            "Makes the photo brighter or darker, measured in stops: +1 doubles brightness and −1 halves it. Unlike Sensor gain, this acts after noise reduction and color processing. Start here for overall brightness. Brightening can reveal noise or turn bright areas white; darkening cannot recover detail lost when the photo was taken."
        }
        "Saturation" => {
            "Adjusts color intensity. 1× leaves it unchanged, 0 removes color, and higher values make colors stronger. Too much can hide subtle color differences or make skin tones unnatural."
        }
        "Contrast" => {
            "Changes the separation between dark and light tones. Higher values give a punchier look; lower values give a softer look. Strong contrast can hide detail in shadows and highlights. 0 keeps the selected color mode's contrast."
        }
        "Shadows" => {
            "Shapes the darker part of the tone curve. Use it to change how much detail you see in dark areas without relying only on Exposure. Opening shadows can reveal grain and color noise. This is a tone adjustment, not local shadow recovery; 0 leaves it unchanged."
        }
        "Highlights" => {
            "Shapes the brighter part of the tone curve. Use it to adjust the appearance of bright surfaces and their tonal separation. It is not highlight recovery: detail lost at capture cannot be brought back by this slider. 0 leaves it unchanged."
        }
        "Reference reconstruction" => {
            "Enables noise reduction followed by detail restoration, using the camera's noise information. Use it to balance smoother areas against fine texture. Too much smoothing can look waxy; too much restored detail can bring noise back. The Chroma noise, Luma noise, and Detail controls work only while this is on."
        }
        "Chroma noise" => {
            "Reduces unwanted colored speckles and blotches, especially in shadows. The five choices are camera-based presets, not a simple strength scale; 2 is the standard starting point, and 0 still reduces noise. Compare smooth areas and fine color texture: cleanup can also remove real color detail."
        }
        "Luma noise" => {
            "Reduces brightness grain rather than colored blotches. The five choices are camera-based presets, not a simple strength scale; 2 is the standard starting point, and 0 still reduces noise. Compare smooth areas with hair, leaves, or fabric: smoother results can lose fine texture."
        }
        "Detail" => {
            "Brings fine texture back after noise reduction. 0 shows the smoothed image, 1× uses the normal amount of restored detail, and higher values restore more. Too much can bring grain and colored speckles back. This is separate from final sharpening."
        }
        "Auto ColorDQ" => {"Uses native processing ISO gain capped by the noise recipe when reconstruction is enabled. Turn this off for a manual gain; manual 0 disables ColorDQ."}
        "ColorDQ gain" => {
            "Reduces small color differences between red, green, and blue within each pixel, using camera-specific settings. It can tame subtle color speckles, but may also weaken genuine fine color differences. 0 turns it off. It is not Saturation and does not smooth neighboring pixels; compare a shadow crop before increasing it."
        }
        "Sharpen" => {
            "Emphasizes edges to make the finished photo look crisper. 0 turns it off. Higher amounts can exaggerate grain and create light or dark outlines around edges. It cannot restore detail that was never recorded."
        }
        "Camera-record repair" => {
            "Fixes faulty sensor pixels that can appear as bright or colored dots, using correction information from the camera. Usually best left on. It repairs individual defects and small faulty patches, not general image grain. Despeckle checks for additional dots separately."
        }
        "Manual cutoff" => {
            "Replaces the camera's automatic choice of which defects to repair. Usually leave this off and let the camera information choose. Manual values are defect ratings, not an overall noise-reduction strength."
        }
        "Min. badness" => {
            "Sets the minimum camera defect rating for individual-pixel repairs. Lower values include more listed pixels; higher values keep only more severe ones. The same number also selects a separate group of larger faulty patches, so it is not a simple repair-strength slider. Usually the automatic camera choice is best."
        }
        "Row banding correction" => {
            "Reduces faint horizontal brightness or color stripes by estimating small per-row sensor offsets. It acts before general noise reduction and does not smooth image texture. Usually best left on; switch it off to compare an affected shadow area."
        }
        "Highlight restoration" => {
            "Attempts to restore natural-looking color in very bright areas where part of the sensor reached its limit. It uses information still recorded by the other color-sensitive layers and can reduce false colors. It cannot recover texture lost in all three layers. This acts before noise reduction and is separate from the Highlights tone slider."
        }
        "Despeckle" => {
            "Finds isolated bright, dark, or colored dots that remain after camera-based repairs, then replaces them using nearby pixels. Usually best left on. It does not reduce general grain; switch it off to check whether a tiny real detail is being removed."
        }
        _ => return None,
    })
}

pub fn navigator(os: OperatingSystem) -> String {
    let modifier = if matches!(os, OperatingSystem::Mac | OperatingSystem::IOS) {
        "⌘"
    } else {
        "Ctrl"
    };
    format!(
        "Drag the navigator's outlined region to pan. Double-click the main image or navigator to switch between Fit and your snap zoom (200% by default). Main-image zoom keeps the clicked point under the cursor; navigator zoom centres the clicked image location. Click 100%, 200%, or 400% to zoom immediately. {modifier}-click a button to select the snap zoom without changing the current view. The colored underline marks the snap target, not the current zoom; your choice is remembered across restarts. Percentages use display pixels per image pixel, including on Retina displays."
    )
}

pub fn scope(mode: Scope) -> &'static str {
    match mode {
        Scope::Histogram => {
            "Shows how much of the image falls at each brightness level: dark on the left, bright on the right. Colored curves show the separate colors. Use it to judge tonal balance. It measures the selected inspection stage, not always the finished photo; reaching an edge does not by itself prove detail was lost at capture."
        }
        Scope::Parade => {
            "Shows red, green, and blue side by side, with dark values at the bottom and bright values at the top. Each panel follows the image from left to right. Use it to compare color balance and spot an uneven color cast. In sensor views, the panels are sensor layers, not finished red, green, and blue colors."
        }
        Scope::Waveform => {
            "Shows where dark and bright areas fall across the image: left to right matches the photo, and higher means brighter. Colors overlap; the white trace shows overall brightness for color images. Useful for finding bright or dark regions. All scopes measure the selected inspection stage, independently of any single-color display view."
        }
    }
}

pub fn inspection(name: &str) -> Option<&'static str> {
    Some(match name {
        "final" => {
            "The finished photo with all enabled edits. This is the result used for export. Choose RGB to see the normal color image."
        }
        "sensor.layers" => {
            "The original recorded sensor readings, before white balance or corrections. This camera records three overlapping color-sensitive layers at each pixel. These grayscale views are not the finished red, green, and blue channels, so a dark or unusual-looking layer is not necessarily a problem."
        }
        "calibration.layers" => {
            "Sensor readings after the camera's black-level corrections and your white balance choice, before Sensor gain and defect repairs. Each layer is shown in gray, not as a finished color channel."
        }
        "gain.layers" => {
            "The corrected sensor readings after Sensor gain, before defect repairs and noise reduction. Use this to see the early brightness adjustment. The layers are not finished red, green, and blue channels."
        }
        "repair.layers" => {
            "Sensor readings after camera-based repairs of faulty pixels and patches. Compare with the earlier stage to inspect a known defect; general grain is not removed here."
        }
        "highlight.layers" => {
            "Sensor readings after an attempt to restore natural-looking color in very bright areas. Compare with repair.layers to inspect highlight color changes. This is separate from the Highlights tone slider and cannot recreate all detail lost at capture."
        }
        "highlight.density" => {
            "A diagnostic map of areas near highlights where several sensor layers reached their limit when the photo was taken. Brighter areas receive stronger highlight treatment. This is not a brightness image or a measure of recovered detail."
        }
        "despeckle.layers" => {
            "Sensor readings after the extra search for isolated bright, dark, or colored dots. Use this to check remaining speckles before general noise reduction."
        }
        "banding.layers" => {
            "Sensor readings after small per-row bias correction. Compare with despeckle to inspect horizontal banding changes before general noise reduction. The three gray layers are not finished color channels."
        }
        "noise.layers" => {
            "The smoothed sensor readings used as the base for detail restoration. Fine texture may look soft here; some is restored later. The three gray layers are not finished color channels."
        }
        "column.layers"=>"Raw sensor layers after column-stripe correction, projected from the processing tiles. This is the input used to estimate brightness/detail texture; the already-filtered base is separate. Useful for checking narrow vertical noise, not final color or tone.",
        "detail.raw-ratio"=>"Brightness/detail ratio before deterministic pattern suppression. Neutral is 1; compare with Detail ratio to see the conditioning rather than a final color image.",
        "pattern.mask"=>"Projected pattern-selection mask: white selects near-neutral ratio texture for deterministic conditioning; black excludes it. Overlap ramps can appear gray. This does not identify camera defects.",
        "noise.edges" => {
            "A diagnostic view of how strongly smoothing can pass through different areas. Brighter values allow more smoothing across an area; darker values restrict it near detected edges. This is a guide for noise reduction, not a photograph."
        }
        "detail.ratio" => {
            "A diagnostic view of the brightness detail to be restored. Middle gray means no brightness change, lighter means a boost, and darker means a reduction. It is not the finished photo."
        }
        "color.rgb" => {
            "The sensor readings converted into red, green, and blue. If noise reduction is enabled, this is the smoothed base before detail restoration. The final contrast and brightness look has not yet been applied, so it can look dark or flat."
        }
        "color-dq.rgb" => {
            "The color image after the extra ColorDQ color cleanup, before detail restoration and the final tone adjustments. Compare fine color texture with color.rgb."
        }
        "reconstruction.rgb" => {
            "The color image after fine detail has been restored to the noise-reduced base. Exposure, the final tone curve, saturation, and sharpening have not yet been applied."
        }
        "detail.signal" => {
            "A diagnostic view of fine brightness variation used by the detail processing. Middle gray means no variation; lighter and darker marks show bright and dark detail. It can contain grain as well as real texture, and is not the finished photo or an exact preview of the Detail slider."
        }
        "tone.rgb" => {
            "The photo after Exposure and the selected color mode's tone curve, including Contrast, Shadows, and Highlights. Additional saturation comes later; optional high-pass sharpening has already been applied before Exposure and tone."
        }
        "saturation.rgb" => {
            "The photo after the Saturation adjustment. Compare it with tone.rgb to judge the change in color intensity."
        }
        "sharpen.rgb" => {
            "Linear RGB after native brightness-proportional high-pass sharpening, before Exposure and tone. Check high-contrast edges for halos and smooth areas for exaggerated grain."
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigator_help_explains_snap_selection_and_platform_modifier() {
        for (os, modifier) in [
            (OperatingSystem::Mac, "⌘-click"),
            (OperatingSystem::Windows, "Ctrl-click"),
            (OperatingSystem::Nix, "Ctrl-click"),
        ] {
            let text = navigator(os);
            for required in [
                modifier,
                "200% by default",
                "without changing the current view",
                "underline",
                "remembered across restarts",
                "under the cursor",
            ] {
                assert!(text.contains(required), "{required}");
            }
        }
    }

    #[test]
    fn every_available_inspection_stage_has_help() {
        let mut settings = foveonic::render::Settings::new("Sunlight", "Standard");
        settings.sensor_gain = 1.3;
        settings.highlight_restoration = true;
        settings.color_dq_gain = Some(1.);
        settings.saturation = 1.2;
        settings.sharpen_amount = 1.;
        settings.reconstruction = Some(Default::default());
        for output in foveonic::inspection::image_outputs(&settings) {
            assert!(inspection(output.name).is_some(), "{}", output.name);
        }
    }

    #[test]
    fn scopes_and_editing_help_explain_important_dependencies() {
        assert!(editing("Sensor gain").unwrap().contains("Exposure"));
        assert!(editing("Exposure").unwrap().contains("Sensor gain"));
        assert!(editing("Sharpen").unwrap().contains("0 turns it off"));
        assert!(editing("Chroma noise").unwrap().contains("0 still"));
        assert!(editing("Luma noise").unwrap().contains("0 still"));
        for mode in [Scope::Histogram, Scope::Parade, Scope::Waveform] {
            assert!(!scope(mode).is_empty());
        }
    }
}
