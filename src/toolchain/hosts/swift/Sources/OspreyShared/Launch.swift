import Foundation

// Implements [NATIVE-HOSTS-CONTRACT] for every entry, macOS and iOS: the launch logic the hosts
// share, with no application state of its own. It registers the faces an application bundled,
// strips the host's own flags from the command line and hands each entry the store every entry
// makes alike — the same adapters, the same arguments, in the same order.

/// The command line the application sees, with the host's own flags accounted for.
public enum HostLaunch {
    /// Prepares the process for the application and returns its own arguments. Faces travel with
    /// the application: they register process-locally before any text draws, and `--resources`
    /// names its folder for media the bundle holds no copy of. Neither flag reaches the
    /// application's own arguments.
    public static func ready(from given: [String]) -> [String] {
        FontRegistration.configureTextRendering()
        FontRegistration.registerBundled()
        if let at = given.firstIndex(of: "--resources"), at + 1 < given.count {
            Bundled.appFolder = URL(fileURLWithPath: given[at + 1], isDirectory: true)
        }
        return arguments(from: given)
    }

    /// The application's own arguments: `--resources` and the folder it names are not among them.
    public static func arguments(from given: [String]) -> [String] {
        var stripped: [String] = []
        var skip = false
        given.forEach { argument in
            if skip { skip = false; return }
            if argument == "--resources" { skip = true; return }
            stripped.append(argument)
        }
        return stripped
    }
}

/// The private store of the application this process runs, keyed to its bundle.
@MainActor
public func bundleStore() -> StoreAdapter {
    StoreAdapter.standard(bundle: Bundle.main.bundleIdentifier ?? "org.ospreylang.host")
}

/// The running application's store, with the capabilities and arguments this launch asks for.
@MainActor
public func makeStore(arguments: [String], adapters: [CapabilityAdapter]) -> RegionStore {
    RegionStore(shared: SharedStore(adapters: adapters), arguments: arguments)
}
