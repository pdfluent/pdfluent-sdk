import Foundation

/// A PDF page boundary box in PDF points (1/72 inch).
public struct PageBox {
    public let x0: Double
    public let y0: Double
    public let x1: Double
    public let y1: Double

    public var width: Double { x1 - x0 }
    public var height: Double { y1 - y0 }
}
