import CoreGraphics
import Foundation
#if canImport(UIKit)
import UIKit
#endif

/// An RGBA-rendered page image.
public struct RenderedImage {
    /// Image width in pixels.
    public let width: Int
    /// Image height in pixels.
    public let height: Int
    /// Raw RGBA pixel data (4 bytes per pixel).
    public let pixels: Data

    /// Convert to a CGImage.
    public func toCGImage() -> CGImage? {
        let bitsPerComponent = 8
        let bytesPerRow = width * 4
        let colorSpace = CGColorSpaceCreateDeviceRGB()
        let bitmapInfo = CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue)

        guard let provider = CGDataProvider(data: pixels as CFData) else {
            return nil
        }

        return CGImage(
            width: width,
            height: height,
            bitsPerComponent: bitsPerComponent,
            bitsPerPixel: 32,
            bytesPerRow: bytesPerRow,
            space: colorSpace,
            bitmapInfo: bitmapInfo,
            provider: provider,
            decode: nil,
            shouldInterpolate: true,
            intent: .defaultIntent
        )
    }

    #if canImport(UIKit)
    /// Convert to a UIImage (iOS/tvOS).
    public func toUIImage() -> UIImage? {
        guard let cgImage = toCGImage() else { return nil }
        return UIImage(cgImage: cgImage)
    }
    #endif

    #if canImport(AppKit)
    /// Convert to an NSImage (macOS).
    public func toNSImage() -> NSImage? {
        guard let cgImage = toCGImage() else { return nil }
        return NSImage(cgImage: cgImage, size: NSSize(width: width, height: height))
    }
    #endif
}
