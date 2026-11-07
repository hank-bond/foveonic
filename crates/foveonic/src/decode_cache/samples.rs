//! Converts decoded sensor codes losslessly between compact disk integers and processing floats.

use crate::source::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Serialize, Deserialize)]
pub(super) enum Encoding {
    Unsigned16,
    Signed16,
}

impl Encoding {
    pub(super) fn for_samples(samples: &[f32]) -> Self {
        if samples.iter().any(|&sample| sample < 0.) {
            Self::Signed16
        } else {
            Self::Unsigned16
        }
    }

    pub(super) fn encode(self, samples: &[f32], output: &mut [u16]) -> Result<()> {
        assert_eq!(samples.len(), output.len());
        for (&sample, code) in samples.iter().zip(output) {
            let restored = match self {
                Self::Unsigned16 => {
                    *code = sample as u16;
                    f32::from(*code)
                }
                Self::Signed16 => {
                    *code = (sample as i16) as u16;
                    f32::from(*code as i16)
                }
            };
            // Cache failures may fall back to decoding, but must never alter a sample.
            if restored.to_bits() != sample.to_bits() {
                return Err(
                    "decoded sample cannot be stored losslessly as a 16-bit integer".into(),
                );
            }
        }
        Ok(())
    }

    pub(super) fn decode(self, codes: &[u16], output: &mut [f32]) {
        assert_eq!(codes.len(), output.len());
        match self {
            Self::Unsigned16 => {
                for (sample, &code) in output.iter_mut().zip(codes) {
                    *sample = f32::from(code);
                }
            }
            Self::Signed16 => {
                for (sample, &code) in output.iter_mut().zip(codes) {
                    *sample = f32::from(code as i16);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_signed_and_unsigned_code_roundtrips_exactly() {
        for encoding in [Encoding::Unsigned16, Encoding::Signed16] {
            let codes: Vec<_> = (u16::MIN..=u16::MAX).collect();
            let mut samples = vec![0.; codes.len()];
            encoding.decode(&codes, &mut samples);
            let mut restored = vec![0; codes.len()];
            Encoding::for_samples(&samples)
                .encode(&samples, &mut restored)
                .unwrap();
            assert_eq!(restored, codes);
        }
    }

    #[test]
    fn unrepresentable_samples_are_rejected_instead_of_rounded() {
        for sample in [-0., 0.5, -32769., 65536., f32::NAN, f32::INFINITY] {
            assert!(Encoding::for_samples(&[sample])
                .encode(&[sample], &mut [0])
                .is_err());
        }
        let mixed = [-1., 65535.];
        assert!(Encoding::for_samples(&mixed)
            .encode(&mixed, &mut [0; 2])
            .is_err());
    }
}
