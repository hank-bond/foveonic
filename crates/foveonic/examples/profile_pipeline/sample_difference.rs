//! Compares complete float streams with bounded storage outside rendering measurements.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Serialize)]
struct Difference {
    samples: u64,
    different_samples: u64,
    nonfinite_samples: u64,
    max_absolute: Option<f64>,
    mean_absolute: Option<f64>,
    rms: Option<f64>,
    quality_sha256: String,
    speed_sha256: String,
}

fn compare(
    mut quality: impl Read,
    mut speed: impl Read,
    bytes: u64,
    little: bool,
) -> Result<Difference, Error> {
    if !bytes.is_multiple_of(4) {
        return Err("processing-mode images have inconsistent float storage".into());
    }
    let mut left = [0u8; 65536];
    let mut right = [0u8; 65536];
    let (mut quality_hash, mut speed_hash) = (Sha256::new(), Sha256::new());
    let (mut changed, mut nonfinite) = (0, 0);
    let (mut maximum, mut absolute_sum, mut squared_sum) = (0f64, 0f64, 0f64);
    let mut remaining = bytes;
    while remaining != 0 {
        let size = remaining.min(left.len() as u64) as usize;
        quality.read_exact(&mut left[..size])?;
        speed.read_exact(&mut right[..size])?;
        quality_hash.update(&left[..size]);
        speed_hash.update(&right[..size]);
        for (a, b) in left[..size]
            .as_chunks::<4>()
            .0
            .iter()
            .zip(right[..size].as_chunks::<4>().0.iter())
        {
            let decode = |v: &[u8; 4]| {
                f64::from(if little {
                    f32::from_le_bytes(*v)
                } else {
                    f32::from_be_bytes(*v)
                })
            };
            let (a, b) = (decode(a), decode(b));
            if !a.is_finite() || !b.is_finite() {
                nonfinite += 1;
                continue;
            }
            let delta = (a - b).abs();
            changed += u64::from(delta != 0.);
            maximum = maximum.max(delta);
            absolute_sum += delta;
            squared_sum += delta * delta;
        }
        remaining -= size as u64;
    }
    if quality.read(&mut left[..1])? != 0 || speed.read(&mut right[..1])? != 0 {
        return Err("processing-mode image length changed during comparison".into());
    }
    let samples = bytes / 4;
    let valid = nonfinite == 0;
    Ok(Difference {
        samples,
        different_samples: changed,
        nonfinite_samples: nonfinite,
        max_absolute: valid.then_some(maximum),
        mean_absolute: valid.then_some(absolute_sum / samples.max(1) as f64),
        rms: valid.then_some((squared_sum / samples.max(1) as f64).sqrt()),
        quality_sha256: format!("{:x}", quality_hash.finalize()),
        speed_sha256: format!("{:x}", speed_hash.finalize()),
    })
}

pub fn run(left: &Path, right: &Path, endianness: &str) -> Result<(), Error> {
    let little = match endianness {
        "little" => true,
        "big" => false,
        _ => return Err("sample endianness must be little or big".into()),
    };
    let (quality, speed) = (File::open(left)?, File::open(right)?);
    let bytes = quality.metadata()?.len();
    if bytes != speed.metadata()?.len() {
        return Err("processing-mode images have inconsistent float storage".into());
    }
    let data = compare(quality, speed, bytes, little)?;
    println!(
        "{}",
        serde_json::json!({"event": "sample_difference", "data": data})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn bytes(values: &[f32], little: bool) -> Vec<u8> {
        values
            .iter()
            .flat_map(|v| {
                if little {
                    v.to_le_bytes()
                } else {
                    v.to_be_bytes()
                }
            })
            .collect()
    }

    #[test]
    fn endian_order_and_signed_zero_preserve_descriptive_statistics() {
        for little in [true, false] {
            let a = bytes(&[1., 2., 3., -0.], little);
            let b = bytes(&[1., 2.5, 2.5, 0.], little);
            let d = compare(Cursor::new(&a), Cursor::new(&b), a.len() as u64, little).unwrap();
            assert_eq!(
                (d.samples, d.different_samples, d.nonfinite_samples),
                (4, 2, 0)
            );
            assert_eq!(d.max_absolute, Some(0.5));
            assert_eq!(d.mean_absolute, Some(0.25));
            assert_eq!(d.rms, Some((0.5f64 / 4.).sqrt()));
            assert_ne!(d.quality_sha256, d.speed_sha256);
        }
    }

    #[test]
    fn nonfinite_values_do_not_report_zero_difference_metrics() {
        let a = bytes(&[f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.], true);
        let b = bytes(&[0., f32::INFINITY, 0., 2.], true);
        let d = compare(Cursor::new(&a), Cursor::new(&b), a.len() as u64, true).unwrap();
        assert_eq!(
            (d.samples, d.different_samples, d.nonfinite_samples),
            (4, 1, 3)
        );
        assert_eq!((d.max_absolute, d.mean_absolute, d.rms), (None, None, None));
    }

    #[test]
    fn incomplete_or_changed_storage_is_rejected() {
        for (a, b, size) in [
            (vec![0; 3], vec![0; 3], 3),
            (vec![0; 4], vec![], 4),
            (vec![0; 8], vec![0; 4], 4),
        ] {
            assert!(compare(Cursor::new(a), Cursor::new(b), size, true).is_err());
        }
    }

    #[test]
    fn multiple_chunks_keep_complete_hashes_and_sample_counts() {
        let a = bytes(&vec![0.; 40000], true);
        let b = bytes(&vec![2.; 40000], true);
        let d = compare(Cursor::new(&a), Cursor::new(&b), a.len() as u64, true).unwrap();
        assert_eq!((d.samples, d.different_samples), (40000, 40000));
        assert_eq!(
            (d.max_absolute, d.mean_absolute, d.rms),
            (Some(2.), Some(2.), Some(2.))
        );
        assert_eq!(d.quality_sha256, format!("{:x}", Sha256::digest(&a)));
        assert_eq!(d.speed_sha256, format!("{:x}", Sha256::digest(&b)));
    }
}
