//! Declares the window's display encoding and tags its native Metal layer for host color matching.

use eframe::{egui_wgpu, wgpu};
use foveonic::source::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// The fragment shader supplies ordinary encoded sRGB.
    SrgbEncoded,
    /// An sRGB attachment encodes the shader's linear output.
    SrgbLinear,
    /// The floating attachment retains extended, encoded sRGB.
    ExtendedSrgbEncoded,
}

#[cfg(test)]
#[path = "display_test.rs"]
mod tests;

impl Target {
    pub fn for_format(format: wgpu::TextureFormat) -> Self {
        if format == wgpu::TextureFormat::Rgba16Float {
            Self::ExtendedSrgbEncoded
        } else if format.is_srgb() {
            Self::SrgbLinear
        } else {
            Self::SrgbEncoded
        }
    }
    pub fn managed(self) -> bool {
        self == Self::ExtendedSrgbEncoded
    }
}

/// Managed macOS windows prefer floating presentation. Unsupported surfaces
/// retain the ordinary sRGB format and its matching display mapping.
pub fn configure(mut config: egui_wgpu::WgpuConfiguration) -> egui_wgpu::WgpuConfiguration {
    if cfg!(target_os = "macos") {
        config.preferred_framebuffer_format = Some(wgpu::TextureFormat::Rgba16Float);
    }
    config
}

pub struct Host {
    pub target: Target,
    #[cfg(target_os = "macos")]
    platform: macos::LayerColor,
}

impl Host {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self> {
        let format = cc
            .wgpu_render_state
            .as_ref()
            .ok_or("missing display device")?
            .target_format;
        let target = Target::for_format(format);
        #[cfg(target_os = "macos")]
        let mut platform = macos::LayerColor::new(target.managed())?;
        #[cfg(target_os = "macos")]
        platform.refresh(cc)?;
        Ok(Self {
            target,
            #[cfg(target_os = "macos")]
            platform,
        })
    }

    /// Recreated surfaces need the same tag. ColorSync handles monitor changes
    /// itself, so there is no monitor-specific image transform to rebuild here.
    pub fn refresh(&mut self, frame: &eframe::Frame) -> Result<()> {
        #[cfg(target_os = "macos")]
        self.platform.refresh(frame)?;
        #[cfg(not(target_os = "macos"))]
        let _ = frame;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::Result;
    use objc2::{
        msg_send,
        runtime::{AnyClass, AnyObject},
    };
    use objc2_core_foundation::{CFRetained, CFString};
    use objc2_core_graphics::{kCGColorSpaceExtendedSRGB, kCGColorSpaceSRGB, CGColorSpace};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    pub(super) struct LayerColor {
        space: CFRetained<CGColorSpace>,
        name: &'static CFString,
        layer: *mut AnyObject,
    }
    impl LayerColor {
        pub fn new(extended: bool) -> Result<Self> {
            // SAFETY: Named spaces are process-lifetime constants. Both names
            // are available since macOS 10.12, below our macOS 15 minimum.
            let name = unsafe {
                if extended {
                    kCGColorSpaceExtendedSRGB
                } else {
                    kCGColorSpaceSRGB
                }
            };
            let space =
                CGColorSpace::with_name(Some(name)).ok_or("cannot create window color space")?;
            Ok(Self {
                space,
                name,
                layer: std::ptr::null_mut(),
            })
        }

        pub fn refresh(&mut self, window: &impl HasWindowHandle) -> Result<()> {
            let RawWindowHandle::AppKit(handle) = window.window_handle()?.as_raw() else {
                return Err("AppKit window required for Metal color matching".into());
            };
            let thread = AnyClass::get(c"NSThread").ok_or("NSThread unavailable")?;
            // SAFETY: NSThread's class method has no object-lifetime requirement.
            let main: bool = unsafe { msg_send![thread, isMainThread] };
            if !main {
                return Err("window color tagging requires the main thread".into());
            }
            let class = AnyClass::get(c"CAMetalLayer").ok_or("CAMetalLayer unavailable")?;
            // SAFETY: The borrowed AppKit handle keeps this NSView alive during
            // this call. AppKit layer access is confined to the main thread.
            let layer = unsafe {
                let view = handle.ns_view.as_ptr().cast::<AnyObject>();
                let root: *mut AnyObject = msg_send![view, layer];
                find_metal(root, class)
            }
            .ok_or("Metal presentation layer unavailable")?;
            if layer != self.layer {
                // SAFETY: find_metal returned a live layer owned by this window.
                // CAMetalLayer retains its colorspace property independently.
                unsafe {
                    let _: () = msg_send![layer, setColorspace: &*self.space];
                }
                // SAFETY: Query the property we just set on the same live layer.
                let matches = unsafe {
                    let actual: *const CGColorSpace = msg_send![layer, colorspace];
                    CGColorSpace::name(actual.as_ref()).is_some_and(|name| &*name == self.name)
                };
                if !matches {
                    return Err("Metal layer did not retain its requested color space".into());
                }
                self.layer = layer;
            }
            Ok(())
        }
    }

    /// The borrowed layer tree must belong to a live NSView on the main thread.
    unsafe fn find_metal(layer: *mut AnyObject, class: &AnyClass) -> Option<*mut AnyObject> {
        if layer.is_null() {
            return None;
        }
        // SAFETY: The caller guarantees live AppKit objects and main-thread access.
        unsafe {
            let matches: bool = msg_send![layer, isKindOfClass: class];
            if matches {
                return Some(layer);
            }
            let children: *mut AnyObject = msg_send![layer, sublayers];
            if children.is_null() {
                return None;
            }
            let count: usize = msg_send![children, count];
            for index in 0..count {
                let child: *mut AnyObject = msg_send![children, objectAtIndex: index];
                if let Some(found) = find_metal(child, class) {
                    return Some(found);
                }
            }
            None
        }
    }
}
