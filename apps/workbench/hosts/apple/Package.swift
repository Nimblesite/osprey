// swift-tools-version: 5.9
import PackageDescription

// The Osprey Workbench for the Mac. `run.sh` compiles `app/` to
// `.build/osprey/macos/libOspreyApp.a` and its generated header, which the
// `OspreyApp` C target exposes; `swift build` alone cannot build the app.
let package = Package(
    name: "Workbench",
    platforms: [.macOS(.v14)],
    products: [.executable(name: "Workbench", targets: ["WorkbenchApp"]), .executable(name: "SnapshotReview", targets: ["SnapshotReview"])],
    targets: [
        .target(name: "OspreyNative", linkerSettings: [.linkedFramework("AVKit")]),
        .testTarget(name: "OspreyNativeTests", dependencies: ["OspreyNative", "OspreyApp"]),
        .executableTarget(name: "SnapshotReview", dependencies: ["OspreyNative"], path: "Tests/SnapshotReview"),
        .target(
            name: "OspreyApp",
            path: "Sources/OspreyApp",
            linkerSettings: [.unsafeFlags(["-L", ".build/osprey/macos"]), .linkedLibrary("OspreyApp")]
        ),
        .executableTarget(name: "WorkbenchApp", dependencies: ["OspreyNative", "OspreyApp"]),
    ]
)
