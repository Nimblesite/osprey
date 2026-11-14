import Foundation
import Darwin

// [ORCHESTRA-NETWORK-TRUST] Prefer a LAN interface for the phone invite; loopback remains useful offline.
func localAddress() throws -> String {
    var first: UnsafeMutablePointer<ifaddrs>?
    guard getifaddrs(&first) == 0 else { throw problem("Cannot discover local network interfaces") }
    defer { freeifaddrs(first) }
    var current = first
    var candidates: [(String, String)] = []
    while let entry = current {
        defer { current = entry.pointee.ifa_next }
        guard let address = entry.pointee.ifa_addr, address.pointee.sa_family == UInt8(AF_INET),
              entry.pointee.ifa_flags & UInt32(IFF_UP) != 0,
              entry.pointee.ifa_flags & UInt32(IFF_LOOPBACK) == 0 else { continue }
        var buffer = [CChar](repeating: 0, count: Int(NI_MAXHOST))
        guard getnameinfo(address, socklen_t(address.pointee.sa_len), &buffer, socklen_t(buffer.count),
                          nil, 0, NI_NUMERICHOST) == 0 else { continue }
        candidates.append((String(cString: entry.pointee.ifa_name), String(cString: buffer)))
    }
    return candidates.sorted { $0.0 < $1.0 }.first(where: { $0.0.hasPrefix("en") })?.1 ?? "127.0.0.1"
}

func validateConfiguration(_ environment: [String: String]) throws {
    if let configured = environment["ORCHESTRA_PORT"], UInt16(configured) == nil {
        throw problem("ORCHESTRA_PORT must be an integer from 1 to 65534")
    }
    if let configured = environment["ORCHESTRA_DURATION"] {
        guard let value = Double(configured), value.isFinite, value >= 0 else {
            throw problem("ORCHESTRA_DURATION must be a nonnegative finite number of seconds")
        }
    }
}
