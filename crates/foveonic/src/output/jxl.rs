//! Owns libjxl encoder resources and forwards photographic samples without changing their colors.

use super::EncodingOptions;
use crate::{render::TiffColorSpace, source::Result};
use jxl_sys::*;
use std::{ffi::c_void, ptr::NonNull};

struct Encoder(NonNull<JxlEncoder>);
impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: this owner frees the encoder exactly once, after all borrowed calls finish.
        unsafe { JxlEncoderDestroy(self.0.as_ptr()) }
    }
}
struct Runner(NonNull<c_void>);
impl Drop for Runner {
    fn drop(&mut self) {
        // SAFETY: the runner outlives the encoder borrowing it and is freed exactly once.
        unsafe { JxlThreadParallelRunnerDestroy(self.0.as_ptr()) }
    }
}

pub(super) fn encode(
    pixels: &[u16],
    width: u32,
    height: u32,
    space: TiffColorSpace,
    options: EncodingOptions,
) -> Result<Vec<u8>> {
    let lossless = options.jxl_lossless();
    // SAFETY: null selects libjxl's default allocator. Both allocations gain unique owners.
    let runner = Runner(
        NonNull::new(unsafe {
            JxlThreadParallelRunnerCreate(
                std::ptr::null(),
                std::thread::available_parallelism().map_or(1, usize::from),
            )
        })
        .ok_or("could not create JPEG XL thread pool")?,
    );
    let encoder = Encoder(
        NonNull::new(unsafe { JxlEncoderCreate(std::ptr::null()) })
            .ok_or("could not create JPEG XL encoder")?,
    );
    let enc = encoder.0.as_ptr();
    let check = |status| -> Result<()> {
        match status {
            JxlEncoderStatus::JXL_ENC_SUCCESS => Ok(()),
            // SAFETY: enc is a live encoder for this entire call.
            _ => Err(format!("JPEG XL encoding failed: {:?}", unsafe {
                JxlEncoderGetError(enc)
            })
            .into()),
        }
    };
    let pixel_format = JxlPixelFormat {
        num_channels: 3,
        data_type: JxlDataType::JXL_TYPE_UINT16,
        endianness: JxlEndianness::JXL_NATIVE_ENDIAN,
        align: 0,
    };
    // SAFETY: all pointers reference live, correctly sized storage. libjxl copies the
    // profile and pixels during these calls. Runner is declared first so it is freed last.
    unsafe {
        check(JxlEncoderSetParallelRunner(
            enc,
            Some(JxlThreadParallelRunner),
            runner.0.as_ptr(),
        ))?;
        let mut info = JxlBasicInfo::default();
        JxlEncoderInitBasicInfo(&mut info);
        info.xsize = width;
        info.ysize = height;
        info.bits_per_sample = 16;
        info.num_color_channels = 3;
        info.uses_original_profile = i32::from(lossless);
        check(JxlEncoderSetBasicInfo(enc, &info))?;
        check(JxlEncoderUseContainer(enc, 1))?;
        if lossless {
            let icc = space.profile();
            check(JxlEncoderSetICCProfile(enc, icc.as_ptr(), icc.len()))?;
        } else {
            // ICC-only XYB files can decode through a clipped sRGB buffer in
            // readers. Native color fields keep the selected gamut available.
            check(JxlEncoderSetColorEncoding(enc, &native_color(space)))?;
        }
        if options.jxl_effort == 11 {
            JxlEncoderAllowExpertOptions(enc);
        }
        let frame = NonNull::new(JxlEncoderFrameSettingsCreate(enc, std::ptr::null()))
            .ok_or("could not create JPEG XL frame")?;
        check(JxlEncoderFrameSettingsSetOption(
            frame.as_ptr(),
            JxlEncoderFrameSettingId::JXL_ENC_FRAME_SETTING_EFFORT,
            options.jxl_effort.into(),
        ))?;
        check(JxlEncoderSetFrameDistance(
            frame.as_ptr(),
            if lossless {
                0.
            } else {
                JxlEncoderDistanceFromQuality(options.jxl_quality.into())
            },
        ))?;
        check(JxlEncoderSetFrameLossless(
            frame.as_ptr(),
            i32::from(lossless),
        ))?;
        check(JxlEncoderAddImageFrame(
            frame.as_ptr(),
            &pixel_format,
            pixels.as_ptr().cast(),
            std::mem::size_of_val(pixels),
        ))?;
        JxlEncoderCloseInput(enc);
    }
    let mut bytes = vec![0_u8; 65536];
    let mut written = 0;
    loop {
        let mut next = bytes[written..].as_mut_ptr();
        let mut remaining = bytes.len() - written;
        // SAFETY: next and remaining describe writable bytes in this allocation.
        let status = unsafe { JxlEncoderProcessOutput(enc, &mut next, &mut remaining) };
        written = bytes.len() - remaining;
        if !matches!(status, JxlEncoderStatus::JXL_ENC_NEED_MORE_OUTPUT) {
            check(status)?;
            bytes.truncate(written);
            return Ok(bytes);
        }
        bytes.resize(
            bytes
                .len()
                .checked_mul(2)
                .ok_or("JPEG XL output is too large")?,
            0,
        );
    }
}

fn native_color(space: TiffColorSpace) -> JxlColorEncoding {
    let mut color = JxlColorEncoding::default();
    // SAFETY: color is writable storage and the initializer copies no pointers.
    unsafe { JxlColorEncodingSetToSRGB(&mut color, 0) };
    match space {
        TiffColorSpace::Srgb => (),
        TiffColorSpace::AdobeRgb => {
            color.primaries = JxlPrimaries::JXL_PRIMARIES_CUSTOM;
            color.primaries_red_xy = [0.64, 0.33];
            color.primaries_green_xy = [0.21, 0.71];
            color.primaries_blue_xy = [0.15, 0.06];
            color.transfer_function = JxlTransferFunction::JXL_TRANSFER_FUNCTION_GAMMA;
            color.gamma = 1. / 2.2;
        }
        TiffColorSpace::ProPhotoRgb => {
            color.white_point = JxlWhitePoint::JXL_WHITE_POINT_CUSTOM;
            color.white_point_xy = [0.3457, 0.3585];
            color.primaries = JxlPrimaries::JXL_PRIMARIES_CUSTOM;
            color.primaries_red_xy = [0.7347, 0.2653];
            color.primaries_green_xy = [0.1596, 0.8404];
            color.primaries_blue_xy = [0.0366, 0.0001];
            color.transfer_function = JxlTransferFunction::JXL_TRANSFER_FUNCTION_GAMMA;
            color.gamma = 1. / 1.8;
        }
    }
    color
}
