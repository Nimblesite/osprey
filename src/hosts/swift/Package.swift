// swift-tools-version: 6.0
import PackageDescription

// The SwiftUI host for Osprey Native applications ([NATIVE-HOSTS-FAMILY]).
// `run.sh` compiles the application to `build/libOspreyApp.a` and its generated
// header, which the `OspreyApp` C target exposes; `swift build` alone cannot
// build a host, because the application is linked in.
let package = Package(
    name: "OspreyHost",
    platforms: [.macOS(.v15), .iOS(.v18)],
    products: [
        .library(name: "OspreyNative", targets: ["OspreyNative"]),
        .executable(name: "OspreyHost", targets: ["OspreyHost"]),
    ],
    targets: [
        .target(name: "OspreyNative"),
        .target(
            name: "OspreyApp",
            path: "Sources/OspreyApp",
            linkerSettings: [.unsafeFlags(["-L", "build/app"]), .linkedLibrary("OspreyApp")]
        ),
        .executableTarget(name: "OspreyHost", dependencies: ["OspreyNative", "OspreyApp"]),
    ],
    swiftLanguageModes: [.v5]
)
