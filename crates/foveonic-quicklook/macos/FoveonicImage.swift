// Wraps pixels from the Rust library as a CGImage. Both Quick Look extensions compile this file.
// FoveonicQuickLook.h is imported as the bridging header.
import CoreGraphics
import Foundation
import OSLog

let log = Logger(subsystem: "org.foveonic.foveonic.quicklook", category: "quicklook")
private let errorCapacity = 4096

struct RenderError: Error, CustomStringConvertible { let description: String }

/// Runs one of the library's render functions and takes ownership of the pixels it returns.
func renderImage(
    _ render: (UnsafeMutablePointer<FoveonicImage>, UnsafeMutablePointer<CChar>, Int) -> Bool
) throws -> CGImage {
    var image = FoveonicImage()
    var error = [CChar](repeating: 0, count: errorCapacity)
    let rendered = withUnsafeMutablePointer(to: &image) { image in
        error.withUnsafeMutableBufferPointer { error in
            render(image, error.baseAddress!, error.count)
        }
    }
    guard rendered else {
        let message = String(cString: error)
        log.error("Quick Look rendering failed: \(message, privacy: .public)")
        throw RenderError(description: message)
    }
    // From here the provider owns the samples and frees them when CoreGraphics is done.
    guard
        let provider = CGDataProvider(
            dataInfo: nil, data: image.samples, size: image.samples_len,
            releaseData: { _, data, size in
                foveonic_ql_samples_free(
                    UnsafeMutablePointer(mutating: data.assumingMemoryBound(to: UInt8.self)), size)
            })
    else {
        foveonic_ql_samples_free(image.samples, image.samples_len)
        throw RenderError(description: "Cannot wrap rendered pixels")
    }
    let profile = Data(bytes: image.profile, count: image.profile_len) as CFData
    let bytesPerPixel = Int(FOVEONIC_QL_BYTES_PER_PIXEL)
    guard
        let space = CGColorSpace(iccData: profile),
        let cgImage = CGImage(
            width: Int(image.width), height: Int(image.height),
            bitsPerComponent: 16, bitsPerPixel: bytesPerPixel * 8,
            bytesPerRow: Int(image.width) * bytesPerPixel, space: space,
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue)
                .union(.byteOrder16Little),
            provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
    else {
        throw RenderError(description: "Cannot wrap rendered pixels")
    }
    return cgImage
}
