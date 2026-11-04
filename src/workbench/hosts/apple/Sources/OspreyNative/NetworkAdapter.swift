import Foundation

// Implements [UI-EFFECTS] and [AGENT-CLIENT]: HTTPS only, no redirects,
// cookies or disk cache, a total resource deadline and a streamed response
// limit [UI-EFFECTS-HTTP-BOUNDS]. Nothing is retried or published partially.

public struct HttpAdapter: CapabilityAdapter {
    public let kinds = ["http"]
    private let transport: URLSession
    public init() { transport = Self.session }
    init(session: URLSession) { transport = session }

    public func perform(_ request: Request, slices: SliceStore) async throws -> JSONValue {
        let timeout = max(try request.number("timeout"), 1)
        let outgoing = try Self.outgoing(request, timeout: timeout)
        let limit = try request.number("limit")
        guard limit >= 0, limit < Int.max else { throw CapabilityError.invalidArgument("Invalid response byte limit") }
        let (data, response) = try await received(outgoing, limit: limit, timeout: timeout)
        try Task.checkCancellation()
        slices.write(try request.text("slice"), try Bounded.text(data))
        return .object(["status": .number(Double(response.statusCode))])
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

    private static func outgoing(_ request: Request, timeout: Int) throws -> URLRequest {
        guard let url = URL(string: try request.text("url")), url.scheme == "https", url.host != nil else { throw CapabilityError.notAllowed("Only https URLs are allowed") }
        var outgoing = URLRequest(url: url, cachePolicy: .reloadIgnoringLocalAndRemoteCacheData, timeoutInterval: Double(timeout) / 1000)
        outgoing.httpMethod = try request.text("method")
        outgoing.httpShouldHandleCookies = false
        request.members["headers"]?.array?.forEach { header in outgoing.setValue(header.object?["value"]?.string, forHTTPHeaderField: header.object?["name"]?.string ?? "") }
        let body = try request.text("body")
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
