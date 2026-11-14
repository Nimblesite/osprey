// swift-tools-version: 6.0
import PackageDescription

// The SwiftUI host for Osprey Native applications ([NATIVE-HOSTS-FAMILY]).
// `view.sh` (and `run.sh`, which opens the same host as a bundled app) compiles
// an application to `build/app/libOspreyApp.a` and its generated header,
// which the `OspreyViewApp` C target exposes; `OspreyShared` leases the frame
// the application writes, checks every row and draws it, and answers the
// capabilities its asks name. `swift build` alone cannot build the host,
// because the application is linked in.
// The folder holding the application's archive: `build/app` for the Mac, and the folder
// `ios.sh` names for an iOS slice, so one platform's archive never answers for another's.
let applicationArchive = Context.environment["OSPREY_APP_ARCHIVE"] ?? "build/app"

let package = Package(
    name: "OspreyHost",
    platforms: [.macOS(.v15), .iOS(.v18)],
    products: [
        .library(name: "OspreyShared", targets: ["OspreyShared"]),
        .executable(name: "OspreyViewHost", targets: ["OspreyViewHost"]),
        .executable(name: "OspreyViewHostIOS", targets: ["OspreyViewHostIOS"]),
    ],
    targets: [
        .target(
            name: "OspreyViewApp",
            path: "Sources/OspreyViewApp",
            linkerSettings: [.unsafeFlags(["-L", applicationArchive]), .linkedLibrary("OspreyApp")]
        ),
        // Films play through AVKit's player view, which nothing loads unless it is linked ([UI-COMPONENTS-MEDIA]).
        .target(name: "OspreyShared", dependencies: ["OspreyViewApp"], linkerSettings: [.linkedFramework("AVKit")]),
        .executableTarget(name: "OspreyViewHost", dependencies: ["OspreyShared", "OspreyViewApp"]),
        .executableTarget(name: "OspreyViewHostIOS", dependencies: ["OspreyShared", "OspreyViewApp"]),
        .testTarget(
            name: "OspreySharedTests",
            dependencies: ["OspreyShared"],
            path: "Tests/OspreySharedTests"
        ),
    ],
    swiftLanguageModes: [.v5]
)
