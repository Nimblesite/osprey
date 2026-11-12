// swift-tools-version: 6.0
import PackageDescription

// The SwiftUI hosts for Osprey Native applications ([NATIVE-HOSTS-FAMILY]).
// `run.sh` compiles an application to `build/app/libOspreyApp.a` and its
// generated header, which the `OspreyApp` C target exposes; `view.sh` compiles
// one to `build/view/libOspreyViewApp.a` for `OspreyViewApp`, whose
// shared-memory rows `OspreyShared` draws. `swift build` alone cannot build a
// host, because the application is linked in.
let package = Package(
    name: "OspreyHost",
    platforms: [.macOS(.v15), .iOS(.v18)],
    products: [
        .library(name: "OspreyNative", targets: ["OspreyNative"]),
        .executable(name: "OspreyHost", targets: ["OspreyHost"]),
        .executable(name: "OspreyViewHost", targets: ["OspreyViewHost"]),
    ],
    targets: [
        .target(name: "OspreyNative"),
        .target(
            name: "OspreyApp",
            path: "Sources/OspreyApp",
            linkerSettings: [.unsafeFlags(["-L", "build/app"]), .linkedLibrary("OspreyApp")]
        ),
        .target(
            name: "OspreyViewApp",
            path: "Sources/OspreyViewApp",
            linkerSettings: [.unsafeFlags(["-L", "build/view"]), .linkedLibrary("OspreyViewApp")]
        ),
        .executableTarget(name: "OspreyHost", dependencies: ["OspreyNative", "OspreyApp"]),
        .target(name: "OspreyShared", dependencies: ["OspreyViewApp"]),
        .executableTarget(name: "OspreyViewHost", dependencies: ["OspreyShared", "OspreyViewApp"]),
    ],
    swiftLanguageModes: [.v5]
)
