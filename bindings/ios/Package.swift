// swift-tools-version:5.7
import PackageDescription

let package = Package(
    name: "XfaPdf",
    platforms: [
        .iOS(.v14),
        .macOS(.v12),
    ],
    products: [
        .library(name: "XfaPdf", targets: ["XfaPdf"]),
    ],
    targets: [
        // Binary XCFramework containing the compiled Rust library.
        // Build with: scripts/build-ios-xcframework.sh
        .binaryTarget(
            name: "PdfCApiFFI",
            path: "PdfCApiFFI.xcframework"
        ),
        .target(
            name: "XfaPdf",
            dependencies: ["PdfCApiFFI"],
            path: "Sources/XfaPdf"
        ),
        .testTarget(
            name: "XfaPdfTests",
            dependencies: ["XfaPdf"],
            path: "Tests/XfaPdfTests"
        ),
    ]
)
