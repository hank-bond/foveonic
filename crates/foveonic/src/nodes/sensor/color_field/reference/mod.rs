//! Adapts the pinned CPU equations to identical floating sensor inputs for tests only.
//!
//! Imported module changes are limited to responsibility/attribution comments,
//! crate import paths, result aliases, and omission of upstream unit tests.
// Keep the oracle's arithmetic and signatures intact rather than refactoring its loops.
#![allow(dead_code, clippy::too_many_arguments, clippy::needless_range_loop)]
use std::sync::atomic::{AtomicBool, Ordering};
pub type ReferenceResult<T> = crate::source::Result<T>;
fn invalid_data(message: &'static str) -> Box<dyn std::error::Error + Send + Sync> {
    message.into()
}
#[derive(Clone, Copy)]
struct Control<'a>(Option<&'a AtomicBool>);
impl<'a> Control<'a> {
    fn none() -> Self {
        Self(None)
    }
    fn check(self) -> ReferenceResult<()> {
        if self.0.is_some_and(|v| v.load(Ordering::Relaxed)) {
            Err("cancelled".into())
        } else {
            Ok(())
        }
    }
}
struct LocalRecovery {
    cols: usize,
    masks: Vec<[u8; 3]>,
    repaired: Vec<bool>,
}
impl LocalRecovery {
    fn mask(&self, row: usize, col: usize) -> [u8; 3] {
        self.masks[row * self.cols + col]
    }
    fn repaired_site(&self, row: usize, col: usize) -> bool {
        self.repaired[row * self.cols + col]
    }
}
struct DngCtx<'a> {
    rows: i32,
    cols: i32,
    prior: *const f64,
    conv_matrix: *mut f64,
    camera_y: Option<[f64; 3]>,
    gradient: Option<&'a gradient_tone::Field>,
    pixels: &'a [[f64; 3]],
}
impl DngCtx<'_> {
    unsafe fn measured(&self, _data: &[u16], _stride: usize, row: usize, col: usize) -> [f64; 3] {
        self.pixels[row * self.cols as usize + col]
    }
}
mod color_field;
mod gradient_tone;
mod highlight_color;
mod tone_anchor;

pub(super) fn run(
    width: usize,
    height: usize,
    pixels: &[[f64; 3]],
    masks: &[[u8; 3]],
    repaired: &[bool],
    neutral: [f64; 3],
    y: [f64; 3],
    matrix: [f64; 9],
) -> ReferenceResult<Vec<[f64; 3]>> {
    let model = LocalRecovery {
        cols: width,
        masks: masks.to_vec(),
        repaired: repaired.to_vec(),
    };
    let mut ctx = DngCtx {
        rows: height as i32,
        cols: width as i32,
        prior: neutral.as_ptr(),
        conv_matrix: matrix.as_ptr() as *mut _,
        camera_y: Some(y),
        gradient: None,
        pixels,
    };
    let gradient =
        unsafe { gradient_tone::Field::build(&ctx, Some(&model), &[], 0, Control::none()) }?;
    ctx.gradient = Some(&gradient);
    let field = unsafe { color_field::Field::build(&ctx, &model, &[], 0, Control::none()) }?;
    let mut result = Vec::with_capacity(pixels.len());
    for row in 0..height {
        for col in 0..width {
            let index = row * width + col;
            let measured = pixels[index];
            let mask = masks[index];
            let mut tone = tone_anchor::recover_severe(measured, mask, neutral, y);
            gradient.apply(row, col, &mut tone, measured, neutral);
            let target = tone.amplitude * neutral.iter().zip(y).map(|(a, b)| a * b).sum::<f64>();
            result.push(
                if let Some(direction) =
                    field.estimate(row, col, measured, mask, repaired[index], target, y)
                {
                    highlight_color::apply(measured, &tone, neutral, y, direction)
                } else {
                    tone.samples
                },
            );
        }
    }
    Ok(result)
}
