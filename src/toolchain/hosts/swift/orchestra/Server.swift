import Foundation
import Network

// [ORCHESTRA-NETWORK-PROTOCOL] This adapter decodes at the edge; Osprey owns admission and gestures.
final class OrchestraServer {
    private let audio: OrchestraAudio
    private let token: String
    private let origin: Set<String>
    private let page: Data
    private var listeners: [NWListener] = []
    private var peers: [Int64: NWConnection] = [:]
    private var authenticated = Set<Int64>()
    private var lastSeen: [Int64: TimeInterval] = [:]
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
        http.newConnectionHandler = { [weak self] in self?.acceptHTTP($0) }
        let parameters = NWParameters.tcp
        let websocket = NWProtocolWebSocket.Options()
        websocket.autoReplyPing = true
        websocket.maximumMessageSize = 4096
        websocket.setClientRequestHandler(.main) { [weak self] protocols, headers in
            let origin = headers.first { $0.name.lowercased() == "origin" }?.value ?? ""
            let accept = protocols.contains("orchestra.v1") && self?.origin.contains(origin) == true
            return NWProtocolWebSocket.Response(status: accept ? .accept : .reject,
                subprotocol: accept ? "orchestra.v1" : nil, additionalHeaders: nil)
        }
        parameters.defaultProtocolStack.applicationProtocols.insert(websocket, at: 0)
        let ws = try NWListener(using: parameters, on: wsPort)
        ws.newConnectionHandler = { [weak self] in self?.acceptWebSocket($0) }
        listeners = [http, ws]
        for listener in listeners {
            listener.stateUpdateHandler = { [weak self] state in
                if case .failed(let error) = state { self?.failure = error.localizedDescription }
            }
            listener.start(queue: .main)
        }
    }

    private func acceptHTTP(_ connection: NWConnection) {
        guard httpConnections.count < 16 else { connection.cancel(); return }
        let key = ObjectIdentifier(connection)
        httpConnections[key] = connection
        connection.start(queue: .main)
        receiveHTTP(connection, collected: Data())
        DispatchQueue.main.asyncAfter(deadline: .now() + 5) { [weak self, weak connection] in
            connection?.cancel()
            self?.httpConnections.removeValue(forKey: key)
        }
    }

    private func receiveHTTP(_ connection: NWConnection, collected: Data) {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 8192) { [weak self] data, _, done, error in
            guard let self else { connection.cancel(); return }
            let request = collected + (data ?? Data())
            if request.count > 8192 || error != nil { self.closeHTTP(connection); return }
            if let text = String(data: request, encoding: .utf8), text.contains("\r\n\r\n") {
                self.replyHTTP(connection, request: text)
            } else if done { self.closeHTTP(connection) }
            else { self.receiveHTTP(connection, collected: request) }
        }
    }

    private func closeHTTP(_ connection: NWConnection) {
        connection.cancel()
        httpConnections.removeValue(forKey: ObjectIdentifier(connection))
    }

    private func replyHTTP(_ connection: NWConnection, request: String) {
        let valid = request.hasPrefix("GET / HTTP/1.")
        let body = valid ? page : Data("Not found".utf8)
        let status = valid ? "200 OK" : "404 Not Found"
        let headers = "HTTP/1.1 \(status)\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: \(body.count)\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n"
        connection.send(content: Data(headers.utf8) + body, completion: .contentProcessed { [weak self] _ in
            self?.closeHTTP(connection)
        })
    }

    private func acceptWebSocket(_ connection: NWConnection) {
        guard peers.count < 16, nextPeer < Int64.max else { connection.cancel(); return }
        nextPeer += 1
        let peer = nextPeer
        peers[peer] = connection
        connection.stateUpdateHandler = { [weak self] state in
            switch state {
            case .failed, .cancelled: self?.disconnect(peer)
            default: break
            }
        }
        connection.start(queue: .main)
        receive(peer, connection)
        DispatchQueue.main.asyncAfter(deadline: .now() + 5) { [weak self] in
            if self?.authenticated.contains(peer) == false { self?.disconnect(peer) }
        }
    }

    private func receive(_ peer: Int64, _ connection: NWConnection) {
        connection.receiveMessage { [weak self] data, context, _, error in
            guard let self, self.peers[peer] != nil else { return }
            let metadata = context?.protocolMetadata(definition: NWProtocolWebSocket.definition) as? NWProtocolWebSocket.Metadata
            guard error == nil, metadata?.opcode == .text, let data, data.count <= 4096 else {
                self.disconnect(peer); return
            }
            self.message(peer, data)

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
                send(peer, ["type": "joined", "peer": peer, "epoch": 1])
                return
            }
            let result = try apply(peer, kind, object)
            if result == 0 { lastSeen[peer] = ProcessInfo.processInfo.systemUptime }
            try audio.update()
            send(peer, ["type": "ack", "accepted": result == 0, "sequence": object["sequence"] ?? 0,
                        "notes": audio.activeNotes, "frames": audio.renderedFrames])
        } catch { send(peer, ["type": "error", "message": error.localizedDescription], close: true) }
    }

    private func apply(_ peer: Int64, _ kind: String, _ object: [String: Any]) throws -> Int64 {
        if kind == "heartbeat" { return osprey_orchestra_heartbeat(peer) }
        guard kind == "touch", let sequence = integer(object["sequence"]),
              let phase = integer(object["phase"]), let finger = integer(object["finger"]),
              let x = coordinate(object["x"]), let y = coordinate(object["y"]),
              x.isFinite, y.isFinite else { throw problem("Invalid controller event") }
        return osprey_orchestra_touch(peer, sequence, phase, finger, x, y)
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
                if close || error != nil { self?.disconnect(peer) }
                else if receiveNext && self?.peers[peer] != nil { self?.receive(peer, connection) }
            })
        } catch { disconnect(peer) }
    }

    private func disconnect(_ peer: Int64) {
        guard let connection = peers.removeValue(forKey: peer) else { return }
        authenticated.remove(peer)
        lastSeen.removeValue(forKey: peer)
        _ = osprey_orchestra_leave(peer)
        do { try audio.update() } catch { failure = error.localizedDescription }
        connection.cancel()
    }

    func silence() throws {
        _ = osprey_orchestra_silence()
        try audio.update()
        for peer in authenticated { send(peer, ["type": "silenced"], receiveNext: false) }
    }

    func expire() {
        let now = ProcessInfo.processInfo.systemUptime
        for (peer, seen) in lastSeen where now - seen >= 1.5 { disconnect(peer) }
    }

    func stop() {
        for peer in Array(peers.keys) { disconnect(peer) }
        for listener in listeners { listener.cancel() }
        for connection in httpConnections.values { connection.cancel() }
        httpConnections.removeAll()
    }
}
