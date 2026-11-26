// Draws the space-bar preview from pixels rendered by the Rust engine.
import CoreGraphics
import QuickLookUI

class PreviewProvider: QLPreviewProvider, QLPreviewingController {
    // Quick Look starts one extension process per preview, so rendering on this
    // task's thread blocks nothing else.
    func providePreview(for request: QLFilePreviewRequest) async throws -> QLPreviewReply {
        let image = try renderImage { image, error, capacity in
            request.fileURL.path.withCString { foveonic_ql_preview($0, image, error, capacity) }
        }
        return QLPreviewReply(
            contextSize: CGSize(width: image.width, height: image.height), isBitmap: true
        ) { context, _ in
            context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        }
    }
}
