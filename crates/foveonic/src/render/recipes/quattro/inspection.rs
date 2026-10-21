//! Describes Quattro inspection outputs and their sensor component meanings.

use crate::{
    inspection::{Channels, ImageOutput},
    source::Metadata,
};

const DECODED: [ImageOutput; 2] = [
    ImageOutput {
        name: "sensor.rgb",
        channels: Channels::Layers,
    },
    ImageOutput {
        name: "sensor.blue",
        channels: Channels::Scalar,
    },
];
const EXTRACTED: [ImageOutput; 2] = [
    ImageOutput {
        name: "extraction.rgb",
        channels: Channels::Layers,
    },
    ImageOutput {
        name: "extraction.blue",
        channels: Channels::Scalar,
    },
];
const PHASE_GAIN: ImageOutput = ImageOutput {
    name: "phase-gain.blue",
    channels: Channels::Scalar,
};

/// Capture identity establishes decoded planes before calibration metadata arrives.
/// Complete metadata determines which additional preparation boundaries are available.
pub(crate) fn outputs(metadata: Option<&Metadata>) -> Vec<ImageOutput> {
    let mut outputs = DECODED.to_vec();
    let Some(metadata) = metadata else {
        return outputs;
    };
    if super::calibration::supports_extraction(metadata) {
        outputs.extend(EXTRACTED);
        if metadata
            .scalar("CAMERAID")
            .is_ok_and(|id| (80. ..=83.).contains(&id))
        {
            outputs.push(PHASE_GAIN);
        }
    }
    outputs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{Matrix, MatrixEncoding};

    #[test]
    fn catalogue_keeps_names_and_component_meanings_together() {
        let early = outputs(None);
        assert_eq!(
            early
                .iter()
                .map(|o| (o.name, o.channels))
                .collect::<Vec<_>>(),
            [
                ("sensor.rgb", Channels::Layers),
                ("sensor.blue", Channels::Scalar)
            ]
        );
        let mut metadata = Metadata {
            width: 5888,
            height: 3672,
            rotation: 0,
            processing_version: Some([3, 1, 2, 0]),
            properties: Default::default(),
            lists: Default::default(),
            matrices: [(
                "CAMERAID".into(),
                Matrix {
                    encoding: MatrixEncoding::Float32,
                    representation: "f64",
                    dimensions: vec![1],
                    values: vec![81.],
                },
            )]
            .into(),
        };
        let full = outputs(Some(&metadata));
        assert_eq!(
            full.iter().map(|o| o.name).collect::<Vec<_>>(),
            [
                "sensor.rgb",
                "sensor.blue",
                "extraction.rgb",
                "extraction.blue",
                "phase-gain.blue"
            ]
        );
        for output in full {
            assert_eq!(
                output.channels,
                if output.name.ends_with(".blue") {
                    Channels::Scalar
                } else {
                    Channels::Layers
                }
            );
        }
        metadata.matrices.remove("CAMERAID");
        assert_eq!(outputs(Some(&metadata)).len(), 4);
        for version in [[3, 0, 0, 0], [3, 0, 0, 1], [4, 0, 1, 0]] {
            metadata.processing_version = Some(version);
            assert_eq!(outputs(Some(&metadata)).len(), 2);
        }
    }
}
