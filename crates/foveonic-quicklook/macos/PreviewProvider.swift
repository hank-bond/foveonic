// Draws the space-bar preview from pixels rendered by the Rust engine.
// FoveonicQuickLook.h is imported as the bridging header.
import CoreGraphics
import Foundation
import OSLog
import QuickLookUI

private let log = Logger(subsystem: "org.foveonic.foveonic.preview", category: "preview")
private let errorCapacity = 4096

struct PreviewError: Error, CustomStringConvertible { let description: String }

class PreviewProvider: QLPreviewProvider, QLPreviewingController {
    // Quick Look starts one extension process per preview, so rendering on this
    // task's thread blocks nothing else.
    func providePreview(for request: QLFilePreviewRequest) async throws -> QLPreviewReply {
        let image = try render(request.fileURL)
        return QLPreviewReply(
            contextSize: CGSize(width: image.width, height: image.height), isBitmap: true
        ) { context, _ in
            context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        }
    }

    private func render(_ url: URL) throws -> CGImage {
        var image = FoveonicImage()
        var error = [CChar](repeating: 0, count: errorCapacity)
        guard url.path.withCString({ foveonic_ql_preview($0, &image, &error, errorCapacity) }) else {
            let message = String(cString: error)
            log.error("Preview failed: \(message, privacy: .public)")
            throw PreviewError(description: message)
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
            throw PreviewError(description: "Cannot wrap rendered pixels")
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
            throw PreviewError(description: "Cannot wrap rendered pixels")
        }
        return cgImage
    }
}
