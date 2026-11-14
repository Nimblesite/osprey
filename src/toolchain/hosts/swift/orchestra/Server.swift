import Foundation
import Network

// [ORCHESTRA-NETWORK-PROTOCOL] This adapter decodes at the edge; Osprey owns admission and gestures.
final class OrchestraServer {
    private let network = DispatchQueue(label: "org.ospreylang.orchestra.network", qos: .userInteractive)
    private let audio: OrchestraAudio
    private let token: String
    private let origin: Set<String>
    private let page: Data
    private var listeners: [NWListener] = []
    private var peers: [Int64: NWConnection] = [:]
    private var authenticated = Set<Int64>()
    private var lastSeen: [Int64: TimeInterval] = [:]
    private var pendingPings = Set<Int64>()
    private let heartbeatInterval = 0.5
    private var lastPing = 0.0
    private var nextPeer: Int64 = 0
    private var httpConnections: [ObjectIdentifier: NWConnection] = [:]
    private(set) var failure: String?
    var controllerCount: Int { authenticated.count }

    init(audio: OrchestraAudio, host: String, token: String, page: Data, port: UInt16) {
        self.audio = audio
        self.token = token
        self.page = page
        origin = Set(["http://\(host):\(port)", "http://127.0.0.1:\(port)", "http://localhost:\(port)"])
    }

    func start(port: UInt16) throws {
        guard port > 0, port < UInt16.max, let httpPort = NWEndpoint.Port(rawValue: port),
              let wsPort = NWEndpoint.Port(rawValue: port + 1) else { throw problem("Invalid server port") }
        let http = try NWListener(using: .tcp, on: httpPort)
        http.newConnectionHandler = { [weak self] connection in performSession { self?.acceptHTTP(connection) } }
        let ws = try NWListener(using: websocketParameters(), on: wsPort)
        ws.newConnectionHandler = { [weak self] connection in performSession { self?.acceptWebSocket(connection) } }
        listeners = [http, ws]
        for listener in listeners {
            listener.stateUpdateHandler = { [weak self] state in
                performSession { if case .failed(let error) = state { self?.failure = error.localizedDescription } }
            }
            listener.start(queue: network)
        }
    }

    private func websocketParameters() -> NWParameters {
        let parameters = NWParameters.tcp
        let websocket = NWProtocolWebSocket.Options()
        websocket.autoReplyPing = true
        websocket.maximumMessageSize = 4096
        websocket.setClientRequestHandler(network) { [origin] protocols, headers in
            let requestOrigin = headers.first { $0.name.lowercased() == "origin" }?.value ?? ""
            let accept = protocols.contains("orchestra.v1") && origin.contains(requestOrigin)
            return NWProtocolWebSocket.Response(status: accept ? .accept : .reject,
                subprotocol: accept ? "orchestra.v1" : nil, additionalHeaders: nil)
        }
        parameters.defaultProtocolStack.applicationProtocols.insert(websocket, at: 0)
        return parameters
    }

    private func acceptHTTP(_ connection: NWConnection) {
        guard httpConnections.count < 16 else { connection.cancel(); return }
        let key = ObjectIdentifier(connection)
        httpConnections[key] = connection
        connection.start(queue: network)
        receiveHTTP(connection, collected: Data())
        network.asyncAfter(deadline: .now() + 5) { [weak self, weak connection] in
            performSession {
                connection?.cancel()
                self?.httpConnections.removeValue(forKey: key)
            }
        }
    }

    private func receiveHTTP(_ connection: NWConnection, collected: Data) {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 8192) { [weak self] data, _, done, error in
            performSession {
                guard let self else { connection.cancel(); return }
                let request = collected + (data ?? Data())
                if request.count > 8192 || error != nil { self.closeHTTP(connection); return }
                do {
                    if let text = try self.completeHTTP(request) { self.replyHTTP(connection, request: text) }
                    else if done { self.closeHTTP(connection) }
                    else { self.receiveHTTP(connection, collected: request) }
                } catch {
                    log("transport.http_rejected", ["reason": error.localizedDescription])
                    self.closeHTTP(connection)
                }
            }
        }
    }

    private func closeHTTP(_ connection: NWConnection) {
        connection.cancel()
        httpConnections.removeValue(forKey: ObjectIdentifier(connection))
    }

    private func completeHTTP(_ data: Data) throws -> String? {
        guard let boundary = data.range(of: Data("\r\n\r\n".utf8)) else { return nil }
        guard let headers = String(data: data[..<boundary.lowerBound], encoding: .utf8) else { throw problem("Invalid HTTP headers") }
        let lengths = headers.components(separatedBy: "\r\n").filter { $0.lowercased().hasPrefix("content-length:") }
        guard lengths.count <= 1 else { throw problem("Duplicate HTTP content length") }
        let length = lengths.first?.dropFirst(15).trimmingCharacters(in: .whitespaces) ?? "0"
        guard let count = Int(length) else { throw problem("Invalid HTTP content length") }
        guard count >= 0, count <= 4096 else { throw problem("HTTP body exceeds diagnostic limit") }
        guard data.count >= boundary.upperBound + count else { return nil }
        guard let request = String(data: data.prefix(boundary.upperBound + count), encoding: .utf8) else { throw problem("Invalid HTTP text") }
        return request
    }

    private func receiveDiagnostics(_ request: String) -> String {
        let parts = request.components(separatedBy: "\r\n\r\n")
        let headers = parts[0].components(separatedBy: "\r\n")
        let requestOrigin = headers.first { $0.lowercased().hasPrefix("origin:") }?.dropFirst(7).trimmingCharacters(in: .whitespaces) ?? ""
        guard origin.contains(requestOrigin), let body = parts.dropFirst().joined(separator: "\r\n\r\n").data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: body) as? [String: Any], object["token"] as? String == token else { return "403 Forbidden" }
        guard let events = object["events"] as? [[String: Any]], events.count <= 16 else { return "400 Bad Request" }
        for event in events {
            guard let name = event["event"] as? String, name.hasPrefix("controller."), name.count <= 64 else { return "400 Bad Request" }
            var fields: [String: Any] = ["source": "browser"]
            for key in ["at", "sequence", "phase", "code", "buffered", "state", "visibility", "peer"] {
                if let number = event[key] as? NSNumber, number.doubleValue.isFinite { fields[key] = number }
            }
            if let reason = event["reason"] as? String { fields["reason"] = String(reason.prefix(160)) }
            log(name, fields)
        }
        return "204 No Content"
    }

    private func replyHTTP(_ connection: NWConnection, request: String) {
        let valid = request.hasPrefix("GET / HTTP/1.")
        let status = request.hasPrefix("POST /diagnostics HTTP/1.") ? receiveDiagnostics(request) : valid ? "200 OK" : "404 Not Found"
        let body = valid ? page : Data()
        let length = status.hasPrefix("204") ? "" : "Content-Length: \(body.count)\r\n"
        let headers = "HTTP/1.1 \(status)\r\nContent-Type: text/html; charset=utf-8\r\n\(length)Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n"
        connection.send(content: Data(headers.utf8) + body, completion: .contentProcessed { [weak self] _ in
            performSession { self?.closeHTTP(connection) }
        })
    }

    private func acceptWebSocket(_ connection: NWConnection) {
        guard peers.count < 16, nextPeer < Int64.max else { connection.cancel(); return }
        nextPeer += 1
        let peer = nextPeer
        peers[peer] = connection
        log("transport.connected", ["peer": peer])
        connection.stateUpdateHandler = { [weak self] state in
            performSession {
                switch state {
                case .failed(let error): self?.disconnect(peer, reason: error.localizedDescription)
                case .cancelled: self?.disconnect(peer, reason: "transport_cancelled")
                default: break
                }
            }
        }
        connection.start(queue: network)
        receive(peer, connection)
        network.asyncAfter(deadline: .now() + 5) { [weak self] in
            performSession { if self?.authenticated.contains(peer) == false { self?.disconnect(peer) } }
        }
    }

    private func receive(_ peer: Int64, _ connection: NWConnection) {
        connection.receiveMessage { [weak self] data, context, _, error in
            performSession {
                guard let self, self.peers[peer] != nil else { return }
                let metadata = context?.protocolMetadata(definition: NWProtocolWebSocket.definition) as? NWProtocolWebSocket.Metadata
                if error == nil, metadata?.opcode == .pong {
                    self.receive(peer, connection)
                    return
                }
                guard error == nil, metadata?.opcode == .text, let data, data.count <= 4096 else {
                    self.disconnect(peer, reason: error?.localizedDescription ?? "invalid_websocket_frame"); return
                }
                self.message(peer, data)
            }
        }
    }

    private func message(_ peer: Int64, _ data: Data) {
        do {
            guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let kind = object["type"] as? String else { throw problem("Malformed message") }
            if !authenticated.contains(peer) {
                guard kind == "join", object["token"] as? String == token,
                      osprey_orchestra_admit(peer) == 0 else { throw problem("Session admission denied") }
                authenticated.insert(peer)
                lastSeen[peer] = ProcessInfo.processInfo.systemUptime
                log("controller.joined", ["peer": peer])
                send(peer, ["type": "joined", "peer": peer, "epoch": 1])
                return
            }
            let result = try apply(peer, kind, object)
            if result == 0 { lastSeen[peer] = ProcessInfo.processInfo.systemUptime }
            if kind != "heartbeat" || result != 0 {
                log(result == 0 ? "controller.input" : "controller.rejected", ["peer": peer, "sequence": integer(object["sequence"]) ?? 0,
                     "phase": integer(object["phase"]) ?? -1, "accepted": result == 0])
            }
            try audio.update()
            send(peer, ["type": "ack", "accepted": result == 0, "sequence": object["sequence"] ?? 0,
                        "notes": audio.activeNotes, "frames": audio.renderedFrames])
        } catch {
            log("controller.rejected", ["peer": peer, "reason": error.localizedDescription])
            send(peer, ["type": "error", "message": error.localizedDescription], close: true)
        }
    }

    private func apply(_ peer: Int64, _ kind: String, _ object: [String: Any]) throws -> Int64 {
        if kind == "heartbeat" { return osprey_orchestra_heartbeat(peer) }
        if kind == "settings" { return try settings(peer, object) }
        guard kind == "touch", let sequence = integer(object["sequence"]),
              let phase = integer(object["phase"]), let finger = integer(object["finger"]),
              let x = coordinate(object["x"]), let y = coordinate(object["y"]),
              x.isFinite, y.isFinite else { throw problem("Invalid controller event") }
        return osprey_orchestra_touch(peer, sequence, phase, finger, x, y)
    }

    private func settings(_ peer: Int64, _ object: [String: Any]) throws -> Int64 {
        guard let sequence = integer(object["sequence"]), let instrument = integer(object["instrument"]),
              let tone = coordinate(object["tone"]), let echo = coordinate(object["echo"]),
              let reverb = coordinate(object["reverb"]), let warp = coordinate(object["warp"]) else {
            throw problem("Invalid sound settings")
        }
        let result = osprey_orchestra_settings(peer, sequence, instrument, tone, echo, reverb, warp)
        if result == 0 {
            audio.updateEffects(peer: peer, echo: echo, reverb: reverb)
            log("controller.sound", ["peer": peer, "sequence": sequence, "instrument": instrument,
                                     "tone": tone, "echo": echo, "reverb": reverb, "warp": warp])
        }
        return result
    }

    private func coordinate(_ value: Any?) -> Double? {
        guard let number = value as? NSNumber, CFGetTypeID(number) != CFBooleanGetTypeID(),
              number.doubleValue.isFinite else { return nil }
        return number.doubleValue
    }

    private func integer(_ value: Any?) -> Int64? {
        guard let number = value as? NSNumber, CFGetTypeID(number) != CFBooleanGetTypeID(),
              number.doubleValue >= 0, number.doubleValue <= 9007199254740991,
              number.doubleValue.rounded() == number.doubleValue else { return nil }
        return number.int64Value
    }

    private func send(_ peer: Int64, _ message: [String: Any], close: Bool = false, receiveNext: Bool = true) {
        guard let connection = peers[peer] else { return }
        do {
            let content = try JSONSerialization.data(withJSONObject: message)
            let metadata = NWProtocolWebSocket.Metadata(opcode: .text)
            let context = NWConnection.ContentContext(identifier: "orchestra", metadata: [metadata])
            connection.send(content: content, contentContext: context, completion: .contentProcessed { [weak self] error in
                performSession {
                    if close || error != nil { self?.disconnect(peer, reason: error?.localizedDescription ?? "protocol_rejected") }
                    else if receiveNext && self?.peers[peer] != nil { self?.receive(peer, connection) }
                }
            })
        } catch { disconnect(peer) }
    }

    private func disconnect(_ peer: Int64, reason: String = "transport_closed") {
        guard let connection = peers.removeValue(forKey: peer) else { return }
        pendingPings.remove(peer)
        log("controller.disconnected", ["peer": peer, "reason": reason])
        authenticated.remove(peer)
        lastSeen.removeValue(forKey: peer)
        _ = osprey_orchestra_leave(peer)
        audio.clearEffects(peer: peer)
        do { try audio.update() } catch { failure = error.localizedDescription }
        connection.cancel()
    }

    func silence() throws {
        log("session.silence", ["notes": audio.activeNotes])
        _ = osprey_orchestra_silence()
        audio.clearEffects()
        try audio.update()
        for peer in authenticated { send(peer, ["type": "silenced"], receiveNext: false) }
    }

    func expire() {
        let now = ProcessInfo.processInfo.systemUptime
        if now - lastPing >= heartbeatInterval {
            for peer in authenticated { ping(peer) }
            lastPing = now
        }
        for (peer, seen) in lastSeen where now - seen >= 1.5 {
            log("controller.expired", ["peer": peer, "heartbeatAgeMs": (now - seen) * 1000])
            disconnect(peer, reason: "heartbeat_timeout")
        }
    }

    // Implements [ORCHESTRA-NETWORK-TIMING]. Native pongs survive coalesced page timers.
    // https://developer.apple.com/documentation/network/nwprotocolwebsocket/metadata/setponghandler(_:handler:)
    private func ping(_ peer: Int64) {
        guard let connection = peers[peer], pendingPings.insert(peer).inserted else { return }
        let metadata = NWProtocolWebSocket.Metadata(opcode: .ping)
        metadata.setPongHandler(network) { [weak self] error in
            performSession { self?.pong(peer, error) }
        }
        let context = NWConnection.ContentContext(identifier: "orchestra-heartbeat", metadata: [metadata])
        connection.send(content: Data(), contentContext: context, completion: .contentProcessed { [weak self] error in
            if let error { performSession { self?.disconnect(peer, reason: error.localizedDescription) } }
        })
    }

    private func pong(_ peer: Int64, _ error: NWError?) {
        pendingPings.remove(peer)
        guard error == nil, authenticated.contains(peer), osprey_orchestra_heartbeat(peer) == 0 else {
            disconnect(peer, reason: error?.localizedDescription ?? "heartbeat_rejected")
            return
        }
        lastSeen[peer] = ProcessInfo.processInfo.systemUptime
    }

    func stop() {
        for peer in Array(peers.keys) { disconnect(peer) }
        for listener in listeners { listener.cancel() }
        for connection in httpConnections.values { connection.cancel() }
        httpConnections.removeAll()
    }
}
