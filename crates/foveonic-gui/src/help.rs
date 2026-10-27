//! Explains editing controls, inspection stages, scopes, and navigator gestures in photographic language.

use crate::preferences::Scope;
use eframe::egui::{self, os::OperatingSystem};

/// Attach the same explanation to editable and read-only controls.
pub fn attach(response: egui::Response, text: &str) -> egui::Response {
    response.on_hover_text(text).on_disabled_hover_text(text)
}

pub fn control(response: egui::Response, label: &str) -> egui::Response {
    match label {
        "Highlight correction" => crate::help_examples::attach(response, "highlight-correction"),
        "Bad pixels" => crate::help_examples::attach(response, "bad-pixels"),
        "Speckles" => crate::help_examples::attach(response, "speckles"),
        "Row banding" => crate::help_examples::attach(response, "row-banding"),
        _ => match editing(label) {
            Some(text) => attach(response, text),
            None => response,
        },
    }
}

/// Explain the automatic/manual toggle in photographic rather than pipeline terms.
pub fn automatic(label: &str) -> Option<&'static str> {
    Some(match label {
        "Highlight correction" => {
            "A uses the camera's suggested setting. Click A to adjust it yourself, or click again to return to Auto. 0 matches Auto. Use the checkbox to turn correction off without losing your setting."
        }
        "Sharpen" => {
            "A uses the camera's suggested sharpening amount for the current noise settings. Click A to choose your own amount. 0 turns sharpening off. Click again to return to Auto."
        }
        "ColorDQ gain" => {
            "A chooses the amount automatically using the camera information, while noise reduction is on. Click A to choose your own amount. 0 turns this correction off. Click again to return to automatic. The slider and value cannot be edited while A is on."
        }
        _ => return None,
    })
}

pub fn editing(label: &str) -> Option<&'static str> {
    Some(match label {
        "Tone method" => "Reference retains the camera family's tone and saturation response. Universal keeps its base camera appearance and RAW exposure, then applies shared contrast and saturation. Reference uses the original Shadows/Highlights across camera families. Universal instead enables the four HDR ranges and Local Contrast. Inactive settings are retained and bypassed. Positive saturation protects already vivid colors. DNG bakes active sensitivity, HDR, Local Contrast and Universal sharpening while retaining camera appearance in metadata. Global tone presentation keeps its existing DNG interpretation. Processing settings can omit sections for each output format.",
        "Local contrast" => "Adjusts texture contrast independently of Noise, Fine/Coarse Detail and sharpening. Negative smooths tonal texture. Positive adds definition. Zero is unchanged and skips filtering. Runs before the camera tone response and bakes into DNG. Keep the X3F and sidecar to change the adjustment.",
        "Sharpening method" => "Reference retains the camera family's sharpening response. Universal replaces its creative sharpening with shared Amount, Radius, Threshold and Halo suppression controls, while sensor reconstruction remains active. The two methods keep independent amounts. Universal runs before the camera tone response and is baked into DNG.",
        "Universal amount" => "Strengthens edges in the finished color or monochrome image. Zero turns it off. Universal replaces reference sharpening instead of adding another sharpening pass. Applies to preview, TIFF and DNG samples.",
        "Universal radius" => "Controls the spatial size of sharpened edges in native-image pixels. Small values emphasize fine edges. Larger values create broader definition and can make outlines visible.",
        "Sharpen threshold" => "Excludes weak brightness differences from sharpening. Increase it to leave smooth skin and noise alone. Zero sharpens all differences.",
        "Halo suppression" => "Restrains bright and dark outlines introduced near strong edges. Higher values limit overshoot more strongly and can soften the sharpening effect.",
        "Red" | "Yellow" | "Green" | "Cyan" | "Blue" | "Magenta" => "Changes brightness according to the subject's original white-balanced color. Positive brightens and negative darkens. The color selection stays independent of the layer mixture. Nearly gray subjects are protected. Color and Monochrome use the same response, baked into DNG samples. Keep the X3F to change a baked adjustment.",

        "White balance" => {
            "Corrects the color cast of the light so whites and grays look neutral. Choose a preset close to the lighting, then judge skin tones and neutral objects. It also affects how color noise is treated."
        }
        "Rendering" => {
            "Color reconstructs a color photograph. Monochrome uses the camera family’s dedicated mixing, filtering and detail processing. Switching does not discard source color."
        }
        "RGB Mix" => {
            "Mixes reconstructed red, green and blue channels before monochrome contrast and detail. Drag inside the triangle or select a channel corner. Reset restores the file's initial proportions."
        }
        "Color Mix" => {
            "Mixes overlapping color-sensitive sensor layers before monochrome reconstruction. Drag within the triangle or select a pure R, G, or B corner. Blue-only can give cleaner high-ISO monochrome. Reset restores equal proportions. DNG bakes this selection. Retain the X3F and sidecar to change it."
        }
        "Mono noise" => {
            "Reduces monochrome grain. Choose 0 for the least smoothing and 4 for the strongest. Strong smoothing can soften fine texture."
        }
        "Mono banding" => {
            "Reduces sensor-column stripes in the mixed signal. Choose 0 to 4. 0 still applies correction. It also influences Speckles independently of the color noise controls."
        }
        "Color mode" => {
            "Chooses the camera's color and contrast look, such as a more natural or vivid rendering. This is a starting look, not just a saturation adjustment. It can also change the tone curve."
        }
        "Exposure" => {
            "Makes the photo brighter or darker, measured in stops: +1 doubles brightness and −1 halves it. This acts after noise reduction and detail processing. Start here for overall brightness. Brightening can reveal noise or turn bright areas white. Darkening cannot recover detail lost when the photo was taken."
        }
        "Saturation" => {
            "Adjusts color intensity. 1× leaves it unchanged, 0 removes color, and higher values make colors stronger. Too much can hide subtle color differences or make skin tones unnatural."
        }
        "Contrast" => {
            "Changes the separation between dark and light tones. Higher values give a punchier look. Lower values give a softer look. Strong contrast can hide detail in shadows and highlights. 0 keeps the starting contrast for the selected rendering mode."
        }
        "Tonal ranges" => {
            "HDR adjustments for Blacks, Shadows, Highlights and Whites. Positive values brighten and negative values darken. Zero preserves the camera look. Surrounding brightness across several scales preserves texture. Values range from -1 to +1. The range pivot follows the camera curve and Exposure. DNG bakes active HDR using the same spatial response as the preview. The camera look stays in metadata."
        }
        "Shadows" => {
            "Shapes the darker part of the tone curve. Use it to change how much detail you see in dark areas without relying only on Exposure. Opening shadows can reveal grain and color noise. This is a tone adjustment, not local shadow recovery. 0 leaves it unchanged."
        }
        "Highlights" => {
            "Shapes the brighter part of the tone curve. Use it to adjust the appearance of bright surfaces and their tonal separation. It is not highlight recovery: detail lost at capture cannot be brought back by this slider. 0 leaves it unchanged."
        }
        "Tone" | "Tone and Color" => {
            "Temporarily bypasses exposure, saturation and tonal edits while preserving their values. The selected camera color mode and its base tone curve remain active. Neutral slider values already leave those edits unchanged."
        }
        "Sharpening" => {
            "Temporarily bypasses final sharpening while preserving its amount and Auto selection. Zero sharpening is already neutral."
        }
        "Noise" => {
            "Enables chromatic, luminance and column noise cleanup. Switching it off preserves your settings and leaves Detail independent. Automatic ColorDQ follows Noise. A manual ColorDQ override remains independent."
        }
        "Detail" => {
            "Enables Fine and Coarse brightness texture adjustments independently of Noise. Switching it off uses neutral detail while preserving your amounts and sizes. At 1 both amounts already recombine neutrally and the spatial detail filters are skipped."
        }
        "Noise and Detail" => {
            "Enables the camera's noise reduction and detail restoration. Too much smoothing can look waxy. Too much restored detail can bring noise back. Shared sensor corrections remain independently selectable."
        }
        "Chroma noise" => {
            "Reduces colored speckles and blotches, especially in shadows. For Merrill, higher positions remove more colored grain independently of texture contrast. Start at 2. The five settings run from 0 to 4. At 0 the added chromatic cleanup is off, while shared sensor corrections remain active. Quattro retains its camera presets. Compare smooth areas and fine color texture: cleanup can also remove real color detail."
        }
        "Luma noise" => {
            "Reduces grain rather than colored blotches. For Merrill, higher positions use more cleaned brightness evidence without adding contrast gain. Fine and Coarse detail set texture restoration separately. Start at 2. At 0 the added brightness cleanup is off, while shared sensor corrections remain active. Quattro retains its camera presets. Compare smooth areas with hair, leaves, or fabric: smoother results can lose fine texture."
        }
        "Banding noise" => {
            "Reduces sensor-column stripes independently of Row banding in Corrections. Snaps to five presets: 2 is the starting point, and 0 still applies correction. Higher positions use a wider, stronger filter to estimate column offsets, which can affect fine texture. Camera rotation can make these stripes appear horizontal."
        }
        "Smooth / Crisp" => {
            "Balances smoother rendering against crisper texture. It selects the camera's linked detail-restoration, local-contrast and sharpening settings. The middle position is the normal starting point. Color and luminance noise remain separate controls."
        }
        "Fine detail" => {
            "Restores the smallest brightness textures after noise cleanup, including hair edges and pores. 0 suppresses this texture, 1 restores it at unit strength, and higher values enhance it. Coarse detail controls broader texture separately. Noise and real texture can occupy the same scale."
        }
        "Fine size" => {
            "Size of the Fine/Coarse boundary. 1 keeps the tuned default. Higher values include broader structures in Fine detail. The two bands share this boundary. Set a detail amount away from 1, then inspect texture.change in Color or mono.texture in Monochrome to see its effect. Double-click to reset."
        }
        "Coarse size" => {
            "Width of the additional blur separating Coarse detail from broad shading. 1 keeps the tuned default. Higher values include broader structures in Coarse detail. Set a detail amount away from 1 and inspect texture.change in Color or mono.texture in Monochrome to see its effect. Both amounts at 1 recombine neutrally regardless of size. Double-click to reset."
        }
        "Coarse detail" => {
            "Restores medium-scale brightness texture and local contrast, including skin relief and fabric. Lower values give a gentler appearance while retaining broad shading. 1 restores unit strength. Higher values emphasize relief. Fine detail controls smaller structures separately."
        }
        "ColorDQ gain" => {
            "Tames fine color speckles using camera information. Too much can also weaken real color detail. 0 turns it off. This is different from Saturation. Compare a shadow crop before increasing it. A chooses the amount automatically while noise reduction is on."
        }
        "Highlight recovery" => {
            "Controls the camera's transition toward neutral color in very bright areas. This is separate from the Highlights brightness adjustment. It cannot recover information that all sensor layers lost."
        }
        "Sharpness" => {
            "Adds edge definition to Quattro reconstruction. 0 keeps the camera's baseline detail processing. Higher values strengthen proportional detail and add sharpening to each color channel."
        }
        "Sharpen" => {
            "Emphasizes edges to make the finished photo look crisper. 0 turns it off. A chooses the amount from the camera and current noise settings. Higher amounts can exaggerate grain and create light or dark outlines around edges. It cannot restore detail that was never recorded."
        }
        "TIFF space" => {
            "Chooses the color space written into TIFF pixels and its matching embedded profile. Camera preserves the stored sRGB or Adobe RGB space. sRGB is convenient for sharing. Adobe RGB and ProPhoto RGB retain wider color ranges for color-managed editing. This does not change the preview or DNG."
        }
        "Bad pixels" => crate::help_examples::text("bad-pixels"),
        "Row banding" => crate::help_examples::text("row-banding"),
        "Highlight correction" => crate::help_examples::text("highlight-correction"),
        "Highlight method" => {
            "Classic uses the existing highlight correction and its camera-relative threshold. Color Field: Local uses 64 local brightness sweeps. Color Field: Global solves the full brightness system for smoother gradients at higher cost. Both share the same color reconstruction and tuning. Both use full-sensor recovery and the supported 8px color grid. Your Classic threshold and shared color-field selectivity strength are retained. The Highlights row below edits the selected method's parameter."
        }
        "Color-field highlights" => {
            "Controls which mixed color evidence can steer highlight reconstruction. The default, 0, keeps the original behavior. Increase the strength toward 1 to reject more inconsistent evidence and reduce color bleeding. Strong settings can also discard real color variation in flowers, fabric or skin. This does not add a neutral tint or change brightness reconstruction. Local and Global share the strength. Double-click resets to 0."
        }
        "Advanced corrections" => {
            "Speckles, Bad pixels, and Row banding default on for new edits. Expand to toggle them individually. Collapsing preserves your settings."
        }
        "Speckles" => crate::help_examples::text("speckles"),
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
        "Drag the navigator's outlined region to pan. Double-click the main image or navigator to switch between Fit and your snap zoom (200% by default). Main-image zoom keeps the clicked point under the cursor. Navigator zoom centres the clicked image location. Click 100%, 200%, or 400% to zoom immediately. {modifier}-click a button to select the snap zoom without changing the current view. The colored underline marks the snap target, not the current zoom. Your choice is remembered across restarts. Percentages use display pixels per image pixel, including on Retina displays."
    )
}

pub fn scope(mode: Scope) -> &'static str {
    match mode {
        Scope::Histogram => {
            "Shows how much of the image falls at each brightness level: dark on the left, bright on the right. Colored curves show the separate colors. Use it to judge tonal balance. It measures the selected inspection stage, not always the finished photo. Reaching an edge does not by itself prove detail was lost at capture."
        }
        Scope::Parade => {
            "Shows red, green, and blue side by side, with dark values at the bottom and bright values at the top. Each panel follows the image from left to right. Use it to compare color balance and spot an uneven color cast. In sensor views, the panels are sensor layers, not finished red, green, and blue colors."
        }
        Scope::Waveform => {
            "Shows where dark and bright areas fall across the image: left to right matches the photo, and higher means brighter. Colors overlap. The white trace shows overall brightness for color images. Useful for finding bright or dark regions. All scopes measure the selected inspection stage, independently of any single-color display view."
        }
    }
}

pub fn inspection(name: &str) -> Option<&'static str> {
    Some(match name {
        "sensor.rgb" => "Original low-resolution red and green sensor layers with blue averaged over each four-phase cell. These are uncalibrated camera codes.",
        "phase-gain.blue" => "Blue after the supplied camera gain maps, resized and applied to each sensor phase. This is a calibration inspection output, before reconstruction.",
        "sensor.blue" => "Original blue sensor readings at full sensor resolution, before calibration. This plane has twice the width and height of sensor.rgb.",
        "extraction.rgb" => "Low-resolution sensor layers after offset and channel-gain extraction. These remain camera codes before later calibration and reconstruction.",
        "extraction.blue" => "Full-resolution blue after offset and channel-gain extraction, before later calibration and reconstruction.",
        "blue-refresh.rgb" => "Low-resolution sensor layers with their blue component refreshed from the full-resolution blue readings. This is preparation for calibration, not finished RGB.",
        "hlp.rgb" => "Sensor layers after sparse blue calibration and camera shading gains. Compare with blue-refresh.rgb to inspect those calibration changes.",
        "hlp.blue" => "Full-resolution blue after sparse-pixel calibration and camera shading gains, before sparse-pixel repair.",
        "hlp.scratch" => "Intermediate sensor samples retained before color-dependent calibration gains. This small diagnostic buffer supports sparse-pixel calibration and is not a complete photograph.",
        "hlp-repair.rgb" => "Calibrated sensor layers after repairing sparse blue sample locations. Compare with hlp.rgb to see that repair.",
        "hlp-repair.blue" => "The full-resolution blue plane after repairing sparse sample locations. Compare with hlp.blue before general defect correction.",
        "phase-detection.rgb" => "Sensor layers after camera-recorded phase-detection pixel correction. These grayscale layer views remain unprocessed camera readings.",
        "phase-detection.blue" => "Full-resolution blue after camera-recorded phase-detection pixel correction, before further blue-plane cleanup.",
        "blue-pattern.blue" => "The blue plane after correcting the camera's aperture-dependent sensor pattern. Compare with phase-detection.blue to inspect the change.",
        "chroma-repair.single" => "Sensor layers after the first camera-recorded color-defect repair pass, before the combined repair pass.",
        "chroma-repair.rgb" => "Sensor layers after the combined camera-recorded color-defect repair. Compare with chroma-repair.single before speckle and cluster correction.",
        "chroma-despeckle.rgb" => "Sensor layers after isolated color-speckle correction. This targets individual outliers before clustered defects are repaired.",
        "chroma-clusters.rgb" => "Sensor layers after camera-recorded groups of color defects have been repaired. These are grayscale sensor layers, not finished color channels.",
        "purple-fringe.rgb" => "Sensor layers after the supported camera's optical purple-fringe correction, before highlight restoration.",
        "luma-repair.directional" => "Directional estimates used to repair defects in the full-resolution blue plane. This is an intermediate guide, not a finished brightness image.",
        "luma-repair.dark-offsets" => "Estimated dark-reference offsets used by blue-plane defect correction. Values describe correction offsets rather than scene brightness.",
        "luma-despeckle.dark-pairs" => "Dark-reference estimates after paired speckle correction, before those estimates are used to repair the blue plane.",
        "luma-repair.blue" => "Full-resolution blue after camera-recorded brightness-defect repair, before isolated speckle correction.",
        "luma-despeckle.blue" => "Full-resolution blue after isolated brightness-speckle correction, before clustered defects are repaired.",
        "luma-clusters.blue" => "Full-resolution blue after camera-recorded brightness-defect clusters have been repaired.",
        "blue-mismatch.blue" => "Full-resolution blue after correcting differences between sensor sample phases. Compare with luma-clusters.blue to inspect the change before reconstruction.",
        "highlight-restoration.recovery-mask" => "White marks pixels with a channel reconstructed near saturation using color relationships elsewhere in the photograph. This is separate from the tone Highlights adjustment.",
        "highlight-restoration.rgb" => "Sensor layers after reconstructing bright clipped areas from the remaining layer information. This is separate from the Highlights tone adjustment and cannot recreate all lost detail.",
        "noise.rgb" => "Noise-filtered low-resolution sensor layers used as the base for Quattro reconstruction. The full-resolution blue detail is combined later.",
        "noise.source-after" => "The companion sensor-layer history retained after noise processing. Reconstruction compares this source information with the filtered base.",
        "shading.rgb" => "The filtered sensor-layer base after spatial color-shading compensation for the selected white balance.",
        "shading.source-after" => "The retained sensor-layer source history after the same spatial color-shading compensation as the filtered base.",
        "shading.blue" => "Full-resolution blue after spatial color-shading compensation for the selected white balance, before reconstruction.",
        "raw-color.rgb" => "Low-resolution sensor-derived color after camera color-response processing. These components are still reconstruction guidance rather than the finished displayed RGB.",
        "sector-color.rgb" => "Color-response guidance after the selected color mode's hue and saturation corrections, before full-resolution reconstruction.",
        "reconstruction.residual" => "Brightness-detail guidance extracted while combining the color base with blue sensor detail. It is a reconstruction diagnostic, not a finished photograph.",
        "residual.rgb" => "Reconstructed RGB after residual-detail filtering and highlight-guided finishing, before local contrast and sharpening.",
        "reconstruction.detail" => "The brightness-detail signal retained for final adaptive sharpening. It can include both real texture and sensor noise.",
        "local-contrast.rgb" => "Reconstructed RGB after local contrast processing, before adaptive sharpening and the final photographic tone response.",
        "crop.rgb" => "The finished photo after its active image crop and orientation have been applied. The surrounding sensor margins are excluded.",
        "red-detail.layers" => {
            "Sensor layers after red-channel detail recovery, before highlight restoration. Only the red sensor layer changes."
        }
        "chroma.small-layers" => {
            "Sensor layers after narrow chroma smoothing. Components remain sensor readings rather than finished RGB."
        }
        "chroma.layers" => {
            "Sensor layers after broader chroma smoothing, before camera color conversion."
        }
        "color.corrected-layers" => {
            "Sensor layers after the nonlinear color-noise correction, retained as reconstruction guidance."
        }
        "color.codes" => {
            "Linear working RGB in camera processing code units, before reconstruction and photographic tone."
        }
        "reconstruction.codes" => {
            "Linear working RGB after detail and noise reconstruction. The camera's ISO selects the reconstruction method."
        }
        "chroma.rgb-codes" => {
            "Linear working RGB after broad recursive chroma smoothing, following detail reconstruction."
        }
        "root.rgb" => {
            "Nonlinear working RGB used for detail and tone processing. Its code values are not linear brightness."
        }
        "mono.mix" => {
            "WB-normalized sensor-layer mixing before monochrome filtering or any color matrix. Single linear brightness component."
        }
        "mono.directional" => {
            "Horizontal range-weighted monochrome means for column correction and zero-noise neighborhood processing."
        }
        "mono.column" => {
            "Mixed monochrome after gated column-offset correction in fixed row blocks."
        }
        "mono.filtered" => {
            "Dedicated 19-square histogram-weighted monochrome noise filtering. No color denoising is applied."
        }
        "sensitivity.color-guide" => "Clean white-balanced color used to classify sensitivity ranges and stabilize their brightness adjustment, independently of the rendering mode or sensor mixture.",
        "sensitivity.rgb" => "Linear color or monochrome after stabilized color sensitivity. Compare with Input to see the brightness correction.",
        "mono.texture" => "Fine and coarse texture adjustments on the cleaned monochrome image. Compare with Input to see the combined detail change.",
        "mono.despeckle" => "Monochrome speckle correction using its dedicated neighborhood plane.",
        "monochrome.rgb" => {
            "Linear monochrome after processing gain, replicated into equal neutral channels. No color matrix or residual color reconstruction is applied."
        }
        "final" => {
            "The finished photo with all enabled edits. This is the result used for export. Choose RGB to see the normal color image."
        }
        "sensor.layers" => {
            "The original recorded sensor readings, before white balance or corrections. This camera records three overlapping color-sensitive layers at each pixel. These grayscale views are not the finished red, green, and blue channels, so a dark or unusual-looking layer is not necessarily a problem."
        }
        "calibration.layers" => {
            "Sensor readings after the camera's black-level corrections and your white balance choice, before defect repairs. Each layer is shown in gray, not as a finished color channel."
        }
        "repair.layers" => {
            "Sensor readings after camera-based repairs of faulty pixels and patches. Compare with the earlier stage to inspect a known defect. General grain is not removed here."
        }
        "highlight.layers" => {
            "Sensor readings after an attempt to restore natural-looking color in very bright areas. Compare with repair.layers to inspect highlight color changes. This is separate from the Highlights tone slider and cannot recreate all detail lost at capture."
        }
        "highlight.density" => {
            "A diagnostic map from the camera's original saturation flags. It guides Classic correction, but is not Color field's continuous reliability or donation strength. This is not a brightness image or a measure of recovered detail."
        }
        "despeckle.layers" => {
            "Sensor readings after the extra search for isolated bright, dark, or colored dots. Use this to check remaining speckles before general noise reduction."
        }
        "banding.layers" => {
            "Sensor readings after small per-row bias correction. Compare with despeckle to inspect horizontal banding changes before general noise reduction. The three gray layers are not finished color channels."
        }
        "noise.layers" => {
            "The smoothed sensor readings used as the base for detail restoration. Fine texture may look soft here. Some is restored later. The three gray layers are not finished color channels."
        }
        "column.layers" => {
            "Raw sensor layers after column-stripe correction, projected from the processing tiles. This is the input used to estimate brightness/detail texture. The already-filtered base is separate. Useful for checking narrow vertical noise, not final color or tone."
        }
        "detail.raw-ratio" => {
            "Brightness/detail ratio before deterministic pattern suppression. Neutral is 1. Compare with Detail ratio to see the conditioning rather than a final color image."
        }
        "pattern.mask" => {
            "Projected pattern-selection mask: white selects near-neutral ratio texture for deterministic conditioning. Black excludes it. Overlap ramps can appear gray. This does not identify camera defects."
        }
        "noise.edges" => {
            "A diagnostic view of how strongly smoothing can pass through different areas. Brighter values allow more smoothing across an area. Darker values restrict it near detected edges. This is a guide for noise reduction, not a photograph."
        }
        "detail.ratio" => {
            "A diagnostic view of the brightness detail to be restored. Middle gray means no brightness change, lighter means a boost, and darker means a reduction. It is not the finished photo."
        }
        "texture.change" => {
            "The brightness gain from the Fine and Coarse adjustments relative to neutral detail. Middle gray means unchanged, lighter means increased, and darker means decreased. Both detail amounts at 1 produce flat gray. This excludes denoising, broad shading, color conversion, and final sharpening. These are multiplicative gains before color conversion, not final RGB differences."
        }
        "color.rgb" => {
            "The sensor readings converted into red, green, and blue. With Merrill noise reduction enabled, texture has already been restored in the sensor layers before this conversion. The final contrast and brightness look has not yet been applied, so it can look dark or flat."
        }
        "color-dq.rgb" => {
            "The color image after the extra ColorDQ color cleanup, before final tone adjustments. Merrill texture restoration is already included. Compare fine color texture with color.rgb."
        }
        "reconstruction.rgb" => {
            "The reconstructed color image projected from its processing tiles. Merrill includes the selected Noise and Detail adjustments before this projection. Exposure, the final tone curve, saturation, and sharpening have not yet been applied."
        }
        "detail.signal" => {
            "A diagnostic view of fine brightness variation used by the detail processing. Middle gray means no variation. Lighter and darker marks show bright and dark detail. It can contain grain as well as real texture, and is not the finished photo or an exact preview of the Detail slider."
        }
        "finish.base" => "The camera's base photographic image before shared finishing edits. Reference sharpening is included only when that method is selected.",
        "linear.sharpen" => "Linear RGB after Universal sharpening and before the camera tone response. Compare with Input to see the sharpening change. This adjustment is baked into DNG samples.",
        "linear.local-contrast" => "Linear RGB after pre-curve Local Contrast. Compare with Input to see texture changes before the camera tone curve. This adjustment is baked into DNG samples.",
        "finish.tone" => "Photographic RGB after universal contrast and saturation. The base camera look remains present.",
        "tonal-ranges.mask" => "Brightness used to select the tonal ranges. Black represents -8 stops below nominal white and white represents nominal white. A multiscale spatial base smooths small details while respecting strong edges.",
        "tonal-ranges.rgb" => "Linear working RGB after the tonal exposure gains, before the camera tone curve. Input/Output and Difference compare it with the actual linear input. Zero adjustments are unchanged.",
        "tone.rgb" => {
            "The photo after Exposure and the selected rendering mode's tone curve, including Contrast, Shadows, and Highlights. Classic also includes its saturation adjustment here. Merrill exposes saturation as a later stage. Sharpening is already included."
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
    fn quattro_inspection_catalogues_have_help_before_and_after_metadata_loads() {
        use foveonic::source::{Matrix, MatrixEncoding, Metadata, ProcessingFamily};
        let settings = foveonic::render::Settings::default();
        for output in foveonic::inspection::image_outputs_with_family(
            &settings,
            None,
            Some(ProcessingFamily::Quattro),
        ) {
            assert!(inspection(output.name).is_some(), "{}", output.name);
        }
        let mut seen = std::collections::HashSet::new();
        for camera in [80., 81., 82., 83., 40., 41.] {
            let metadata = Metadata {
                width: if camera == 41. { 6656 } else { 5888 },
                height: if camera == 41. {
                    4480
                } else if camera == 40. {
                    3776
                } else {
                    3672
                },
                rotation: 0,
                processing_version: Some(if camera == 41. {
                    [4, 0, 1, 0]
                } else {
                    [3, 1, 2, 0]
                }),
                properties: [("CAMMODEL".into(), "SIGMA dp2 Quattro".into())].into(),
                matrices: [
                    ("CAMERAID", vec![camera]),
                    ("CamsysVersion", vec![0., 3., 2.]),
                    ("CaptureISO", vec![100.]),
                    ("CaptureAperture", vec![4.]),
                ]
                .into_iter()
                .map(|(name, values)| {
                    (
                        name.into(),
                        Matrix {
                            encoding: MatrixEncoding::Float32,
                            representation: "native",
                            dimensions: vec![values.len()],
                            values,
                        },
                    )
                })
                .collect(),
                lists: [("ColorShadingGainRatio".into(), Default::default())].into(),
            };
            for output in foveonic::inspection::image_outputs(&settings, Some(&metadata)) {
                assert!(
                    inspection(output.name).is_some(),
                    "camera {camera}: {}",
                    output.name
                );
                seen.insert(output.name);
            }
        }
        for stage in [
            "hlp.scratch",
            "luma-repair.dark-offsets",
            "shading.source-after",
            "sector-color.rgb",
            "purple-fringe.rgb",
            "blue-pattern.blue",
            "final",
        ] {
            assert!(seen.contains(stage), "coverage omitted {stage}");
        }
    }

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
        settings.highlight_restoration = true;
        settings.color_dq_gain = Some(1.);
        settings.saturation = 1.2;
        settings.sharpen_amount = Some(1.);
        settings.reconstruction = Some(Default::default());
        for output in foveonic::inspection::image_outputs(&settings, None) {
            assert!(inspection(output.name).is_some(), "{}", output.name);
        }
    }

    #[test]
    fn scopes_and_editing_help_explain_important_dependencies() {
        assert!(editing("Exposure")
            .unwrap()
            .contains("after noise reduction"));
        assert!(editing("Sharpen").unwrap().contains("0 turns it off"));
        assert!(editing("Chroma noise").unwrap().contains("cleanup is off"));
        assert!(editing("Luma noise").unwrap().contains("cleanup is off"));
        let repair = editing("Bad pixels").unwrap();
        assert!(repair.contains("information recorded by the camera"));
        assert!(repair.contains("small patches"));
        assert!(repair.contains("Speckles"));
        assert!(editing("Camera-record repair").is_none());
        assert!(editing("Min. badness").is_none());
        assert!(automatic("Min. badness").is_none());
        let recovery = editing("Highlight correction").unwrap();
        assert!(editing("Highlights").unwrap().contains("tone curve"));
        assert_ne!(editing("Highlights"), editing("Highlight correction"));
        assert!(recovery.contains("+1 starts correction one stop later"));
        assert!(recovery.contains("Move it right to only correct brighter"));
        assert!(recovery.contains("flat patches"));
        assert!(recovery.contains("0 matches Auto, not Off"));
        assert!(recovery.contains("Highlights tone slider"));
        assert!(automatic("Highlight correction")
            .unwrap()
            .contains("checkbox"));
        for mode in [Scope::Histogram, Scope::Parade, Scope::Waveform] {
            assert!(!scope(mode).is_empty());
        }
    }
}

#[test]
fn classic_inspection_stages_have_help_and_layer_components() {
    use foveonic::source::{Matrix, MatrixEncoding, Metadata};
    let mut metadata = Metadata {
        processing_version: None,
        width: 2304,
        height: 1531,
        rotation: 0,
        properties: [
            ("CAMMODEL".into(), "SIGMA SD10".into()),
            ("ISO".into(), "100".into()),
        ]
        .into(),
        matrices: [(
            "ChromaBlur".into(),
            Matrix {
                encoding: MatrixEncoding::Float32,
                representation: "native",
                dimensions: vec![1],
                values: vec![128.],
            },
        )]
        .into(),
        lists: Default::default(),
    };
    let mut settings = foveonic::render::Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(Default::default());
    for iso in ["100", "400"] {
        metadata.properties.insert("ISO".into(), iso.into());
        for output in foveonic::inspection::image_outputs(&settings, Some(&metadata)) {
            assert!(inspection(output.name).is_some(), "{}", output.name);
            if output.name.ends_with("layers") {
                assert_eq!(output.channels, foveonic::inspection::Channels::Layers);
            }
        }
    }
}
