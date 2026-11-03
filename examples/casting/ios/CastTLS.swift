import Foundation
import Network
import Security

// [CAST-MOBILE-DELIVERY] Cast uses self-signed receiver certificates. This
// unverified TLS transport is only for discovered local receivers, never HTTPS.
private final class CastSocket: @unchecked Sendable {
    let connection: NWConnection
    let changed = NSCondition()
    var bytes = Data()
    var status: Int64 = 0
    private let queue = DispatchQueue(label: "casting.tls")
    private static let limit = 1 << 24

    init(host: String, port: NWEndpoint.Port) {
        let tls = NWProtocolTLS.Options()
        sec_protocol_options_set_verify_block(tls.securityProtocolOptions, { _, _, complete in complete(true) }, queue)
        connection = NWConnection(host: NWEndpoint.Host(host), port: port, using: NWParameters(tls: tls))
    }

    func open(milliseconds: Int64) -> Int64 {
        let ready = DispatchSemaphore(value: 0)
        connection.stateUpdateHandler = { [weak self] state in self?.opened(state, ready: ready) }
        connection.start(queue: queue)
        guard ready.wait(timeout: .now() + .milliseconds(Int(milliseconds))) == .success,
              connection.state == .ready else {
            let failure = openingFailure()
            connection.cancel()
            return failure
        }
        receive()
        return 0
    }

    private func opened(_ state: NWConnection.State, ready: DispatchSemaphore) {
        switch state {
        case .ready, .cancelled: ready.signal()
        case .failed: ended(3); ready.signal()
        default: break
        }
    }

    // [CAST-MOBILE-DELIVERY] Keep transport errors distinct from TLS failures.
    private func openingFailure() -> Int64 {
        switch connection.state {
        case let .failed(error), let .waiting(error):
            switch error {
            case .dns: return -2
            case .tls: return -7
            default: return -4
            }
        default: return -4
        }
    }

    private func ended(_ code: Int64) {
        changed.lock()
        status = code
        changed.broadcast()
        changed.unlock()
    }

    private func receive() {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 65536) { [weak self] data, _, complete, error in
            guard let self else { return }
            changed.lock()
            if let data {
                if bytes.count + data.count > Self.limit { status = 3 }
                else { bytes.append(data) }
            }
            if error != nil { status = 3 } else if complete { status = 2 }
            let continuing = status <= 1
            changed.broadcast()
            changed.unlock()
            if continuing { receive() } else { connection.cancel() }
        }
    }

    func read(_ count: Int, milliseconds: Int64) -> String {
        guard count > 0, count <= Self.limit, milliseconds >= 0, milliseconds <= 60000 else { ended(3); return "" }
        let deadline = Date(timeIntervalSinceNow: Double(milliseconds) / 1000)
        changed.lock()
        defer { changed.unlock() }
        if status == 1 { status = 0 }
        while bytes.count < count && status == 0 {
            if !changed.wait(until: deadline) { status = 1; return "" }
        }
        guard bytes.count >= count else { return "" }
        let result = bytes.prefix(count).map { String(format: "%02x", $0) }.joined()
        bytes.removeFirst(count)
        return result
    }

    func write(_ hex: String) -> Int64 {
        let characters = Array(hex.utf8)
        guard characters.count % 2 == 0, characters.count <= Self.limit * 2 else { ended(3); return -1 }
        var data = Data()
        for index in stride(from: 0, to: characters.count, by: 2) {
            guard let byte = UInt8(String(decoding: characters[index...index + 1], as: UTF8.self), radix: 16) else { ended(3); return -1 }
            data.append(byte)
        }
        let done = DispatchSemaphore(value: 0)
        connection.send(content: data, completion: .contentProcessed { [weak self] error in
            if error != nil { self?.ended(3) }
            done.signal()
        })
        guard done.wait(timeout: .now() + .seconds(5)) == .success else { ended(3); connection.cancel(); return -1 }
        changed.lock()
        defer { changed.unlock() }
        return status >= 2 ? -1 : 0
    }

    func close() { ended(2); connection.cancel() }
}

private final class CastTLS: @unchecked Sendable {
    static let shared = CastTLS()
    let lock = NSLock()
    var sockets: [Int64: CastSocket] = [:]
    var returned: UnsafeMutablePointer<CChar>?

    func socket(_ handle: Int64) -> CastSocket? { lock.withLock { sockets[handle] } }

    func text(_ value: String) -> UnsafePointer<CChar>? {
        lock.withLock {
            free(returned)
            returned = strdup(value)
            return returned.map { UnsafePointer($0) }
        }
    }
}

@_cdecl("osprey_tls_connect")
func castConnect(_ host: UnsafePointer<CChar>?, _ port: Int64, _ timeout: Int64) -> Int64 {
    guard let host, port > 0, port <= 65535, timeout > 0, timeout <= 60000,
          let endpoint = NWEndpoint.Port(rawValue: UInt16(port)) else { return -1 }
    let hostname = String(cString: host)
    guard !hostname.isEmpty else { return -1 }
    let socket = CastSocket(host: hostname, port: endpoint)
    let failure = socket.open(milliseconds: timeout)
    guard failure == 0 else { return failure }
    return CastTLS.shared.lock.withLock {
        guard let handle = (Int64(1)...64).first(where: { CastTLS.shared.sockets[$0] == nil }) else { socket.close(); return -5 }
        CastTLS.shared.sockets[handle] = socket
        return handle
    }
}

@_cdecl("osprey_tls_send_hex")
func castWrite(_ handle: Int64, _ hex: UnsafePointer<CChar>?) -> Int64 {
    guard let hex, let socket = CastTLS.shared.socket(handle) else { return -1 }
    return socket.write(String(cString: hex))
}

@_cdecl("osprey_tls_recv_hex")
func castRead(_ handle: Int64, _ count: Int64, _ timeout: Int64) -> UnsafePointer<CChar>? {
    CastTLS.shared.text(CastTLS.shared.socket(handle)?.read(Int(count), milliseconds: timeout) ?? "")
}

@_cdecl("osprey_tls_status")
func castStatus(_ handle: Int64) -> Int64 {
    guard let socket = CastTLS.shared.socket(handle) else { return -1 }
    return socket.changed.withLock { socket.status }
}

@_cdecl("osprey_tls_close")
func castClose(_ handle: Int64) -> Int64 {
    guard let socket = CastTLS.shared.lock.withLock({ CastTLS.shared.sockets.removeValue(forKey: handle) }) else { return -1 }
    socket.close()
    return 0
}

@_cdecl("osprey_net_local_address")
func castRoute(_ host: UnsafePointer<CChar>?, _ port: Int64) -> UnsafePointer<CChar>? {
    guard let host, port > 0, port <= 65535, let endpoint = NWEndpoint.Port(rawValue: UInt16(port)) else { return CastTLS.shared.text("") }
    let connection = NWConnection(host: NWEndpoint.Host(String(cString: host)), port: endpoint, using: .udp)
    let ready = DispatchSemaphore(value: 0)
    connection.stateUpdateHandler = { state in
        switch state { case .ready, .failed, .cancelled: ready.signal(); default: break }
    }
    connection.start(queue: DispatchQueue(label: "casting.route"))
    defer { connection.cancel() }
    guard ready.wait(timeout: .now() + .seconds(3)) == .success,
          case let .hostPort(address, _)? = connection.currentPath?.localEndpoint else { return CastTLS.shared.text("") }
    switch address {
    case let .ipv4(ip): return CastTLS.shared.text(String(describing: ip))
    case let .ipv6(ip): return CastTLS.shared.text(String(describing: ip))
    default: return CastTLS.shared.text("")
    }
}
