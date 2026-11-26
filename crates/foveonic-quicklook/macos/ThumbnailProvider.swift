// Draws Finder thumbnails from the camera's embedded image, without the GPU engine.
import CoreGraphics
import QuickLookThumbnailing

class ThumbnailProvider: QLThumbnailProvider {
    override func provideThumbnail(
        for request: QLFileThumbnailRequest,
        _ handler: @escaping (QLThumbnailReply?, Error?) -> Void
    ) {
        let maximum = request.maximumSize
        // The request is in points; the library fits pixels.
        let side = UInt32((max(maximum.width, maximum.height) * request.scale).rounded(.up))
        do {
            let image = try renderImage { image, error, capacity in
                request.fileURL.path.withCString {
                    foveonic_ql_thumbnail($0, side, image, error, capacity)
                }
            }
            let pixels = CGSize(width: image.width, height: image.height)
            let fit = min(
                1, maximum.width * request.scale / pixels.width,
                maximum.height * request.scale / pixels.height)
            let size = CGSize(
                width: pixels.width * fit / request.scale,
                height: pixels.height * fit / request.scale)
            handler(
                QLThumbnailReply(contextSize: size) { context in
                    // Fill whatever area the context has. Quick Look's context is in
                    // pixels, not the points of contextSize, so a point-sized rect
                    // covered only a quarter of a Retina thumbnail.
                    context.draw(image, in: context.boundingBoxOfClipPath)
                    return true
                }, nil)
        } catch {
            handler(nil, error)
        }
    }
}
