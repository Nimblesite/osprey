import Foundation

// Implements [IOS-VERIFICATION]: these assertions execute inside the iOS app.
enum SmokeCheck {
    static func run(startupStatus: Int32) {
        let checks = initializationChecks(startupStatus) + behaviorChecks()
        let failures = checks.compactMap { $0.1 ? nil : $0.0 }
        let result = failures.isEmpty ? "OSPREY_IOS_SMOKE_OK\n" : "FAIL: \(failures.joined(separator: ", "))\n"
        do {
            let directory = try FileManager.default.url(for: .documentDirectory, in: .userDomainMask,
                                                        appropriateFor: nil, create: true)
            try result.write(to: directory.appendingPathComponent("smoke-result.txt"),
                             atomically: true, encoding: .utf8)
            print(result, terminator: "")
        } catch {
            print("Could not write iOS smoke result: \(error)")
        }
    }

    private static func initializationChecks(_ startupStatus: Int32) -> [(String, Bool)] {
        let initialCalls = HostLog.calls
        let secondStatus = osprey_main()
        return [
            ("initialization", startupStatus == 0 && secondStatus == 0),
            ("initialize once", initialCalls == 1 && HostLog.calls == initialCalls),
            ("initial host callback", HostLog.message == "Osprey initialized"),
            ("string return", ospreyString(osprey_greeting()) == "Osprey on iPhone")
        ]
    }

    private static func behaviorChecks() -> [(String, Bool)] {
        let start = osprey_reset()
        let count = (0..<5).reduce(start) { value, _ in osprey_increment(value) }
        let callbackStatus = osprey_notify(count)
        return [
            ("integer return", start == 0 && count == 5),
            ("checked overflow", osprey_increment(Int64.max) == Int64.max),
            ("decrement floor", osprey_decrement(0) == 0 && osprey_decrement(count) == 4),
            ("boolean return", !osprey_milestone(4) && osprey_milestone(count)),
            ("allocated string return", ospreyString(osprey_summary(count)) == "Count: 5"),
            ("Swift host callback", callbackStatus == 8 && HostLog.message == "Count: 5")
        ]
    }
}
