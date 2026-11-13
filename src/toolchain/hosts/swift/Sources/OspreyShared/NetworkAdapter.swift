import Foundation

// Implements [UI-EFFECTS] and [AGENT-PROVIDERS]: HTTPS, or HTTP on this machine, no redirects,
// cookies or disk cache, a total resource deadline and a streamed response limit [UI-EFFECTS].
// Nothing is retried or published partially. The request's members arrive as the typed fields
// of its ask rows: method 1, URL 2, headers 3, body 4, limit 5, deadline 6, slice 7.

public struct HttpAdapter: CapabilityAdapter {
    public let kinds = ["http"]
    private let transport: URLSession
    public init() { transport = Self.session }
    init(session: URLSession) { transport = session }

    public func perform(_ ask: Ask, slices: SliceStore) async throws -> AskResult {
        let timeout = max(try required(ask, whole: 6, "deadline"), 1)
        let outgoing = try Self.outgoing(ask, timeout: timeout)
        let limit = try required(ask, whole: 5, "limit")
        guard limit >= 0, limit < Int.max else { throw CapabilityError.invalidArgument("Invalid response byte limit") }
        let (data, response) = try await received(outgoing, limit: limit, timeout: timeout)
        let receipt = try Self.receipt(response)
        try Task.checkCancellation()
        slices.write(try required(ask, text: 7, "slice"), try Bounded.text(data))
        return .document(receipt)
    }

    // [UI-EFFECTS] Raw bounded transport metadata; the core interprets it.
    private static func receipt(_ response: HTTPURLResponse) throws -> ResultDocument {
        let fields = response.allHeaderFields
        guard fields.count <= 100 else { throw CapabilityError.tooLarge(fields.count, 100) }
        var bytes = 0
        var headers: [ResultDocument] = []
        for (key, raw) in fields {
            guard let name = key as? String, let value = raw as? String else {
                throw CapabilityError.platform("HTTP response metadata is not textual")
            }
            bytes += name.utf8.count + value.utf8.count
            guard bytes <= 16384 else { throw CapabilityError.tooLarge(bytes, 16384) }
            headers.append(.object(["name": .string(name), "value": .string(value)]))
        }
        let sorted = headers.sorted { ($0.object?["name"]?.string ?? "") < ($1.object?["name"]?.string ?? "") }
        return .object(["status": .number(Double(response.statusCode)), "headers": .array(sorted)])
    }

    private func received(_ outgoing: URLRequest, limit: Int, timeout: Int) async throws -> (Data, HTTPURLResponse) {
        let configuration = transport.configuration
        configuration.timeoutIntervalForResource = Double(timeout) / 1000
        configuration.timeoutIntervalForRequest = Double(timeout) / 1000
        let bounded = URLSession(configuration: configuration, delegate: transport.delegate, delegateQueue: nil)
        defer { bounded.invalidateAndCancel() }
        do {
            let (bytes, response) = try await bounded.bytes(for: outgoing, delegate: bounded.delegate as? URLSessionTaskDelegate)
            defer { bytes.task.cancel() }
            guard let http = response as? HTTPURLResponse else { throw CapabilityError.platform("No HTTP response") }
            return (try await Self.collect(bytes, limit: limit), http)
        } catch let error as URLError where error.code == .timedOut {
            throw CapabilityError.timedOut(timeout)
        }
    }

    /// Plain HTTP stays on this machine: nothing leaves it unencrypted, and local MCP servers are reachable.
    static func isLoopback(_ url: URL) -> Bool {
        url.scheme == "http" && ["localhost", "127.0.0.1", "::1"].contains(url.host ?? "")
    }

    static func outgoing(_ ask: Ask, timeout: Int) throws -> URLRequest {
        guard let url = URL(string: try required(ask, text: 2, "url")), url.scheme == "https" && url.host != nil || isLoopback(url) else { throw CapabilityError.notAllowed("Only https URLs, or http on this machine, are allowed") }
        var outgoing = URLRequest(url: url, cachePolicy: .reloadIgnoringLocalAndRemoteCacheData, timeoutInterval: Double(timeout) / 1000)
        outgoing.httpMethod = try required(ask, text: 1, "method")
        outgoing.httpShouldHandleCookies = false
        ask.pairs(3).forEach { header in outgoing.setValue(header.1, forHTTPHeaderField: header.0) }
        let body = try required(ask, text: 4, "body")
        if !body.isEmpty { outgoing.httpBody = Data(body.utf8) }
        return outgoing
    }

    private static func collect(_ bytes: URLSession.AsyncBytes, limit: Int) async throws -> Data {
        var data = Data()
        for try await byte in bytes {
            try Task.checkCancellation()
            guard data.count < limit else { throw CapabilityError.tooLarge(data.count + 1, limit) }
            data.append(byte)
        }
        return data
    }

    static let session: URLSession = {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.httpCookieStorage = nil
        configuration.urlCache = nil
        configuration.httpShouldSetCookies = false
        return URLSession(configuration: configuration, delegate: NoRedirects(), delegateQueue: nil)
    }()
}

/// Redirects are reported as the status Osprey sees, never followed to another host.
final class NoRedirects: NSObject, URLSessionTaskDelegate {
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) {
        completionHandler(nil)
    }
}
