// swift-tools-version: 6.0
import PackageDescription

// The host's adapters under test. A package of its own because the host package
// builds an executable that links an application, which `swift test` would need.
let package = Package(
    name: "OspreyHostTests",
    platforms: [.macOS(.v15)],
    dependencies: [.package(path: "..")],
    targets: [
        .testTarget(name: "OspreyNativeTests", dependencies: [.product(name: "OspreyNative", package: "swift")], path: "OspreyNativeTests"),
    ],
    swiftLanguageModes: [.v5]
)
