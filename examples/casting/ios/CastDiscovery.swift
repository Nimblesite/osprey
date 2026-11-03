import Foundation

// Native Bonjour discovery; the transcript is consumed by chromecast::Discovery.
// Implements [CAST-MOBILE-DELIVERY]. No subprocess is available on iOS.
@MainActor
final class CastDiscovery: NSObject, @MainActor NetServiceBrowserDelegate, @MainActor NetServiceDelegate {
    private var browser = NetServiceBrowser()
    private var services: [NetService] = []
    private var records: [String: String] = [:]
    private var completion: ((Result<String, Error>) -> Void)?
    private var deadline: Task<Void, Never>?

    func start(completion: @escaping (Result<String, Error>) -> Void) {
        cancel()
        self.completion = completion
        records.removeAll()
        browser = NetServiceBrowser()
        browser.delegate = self
        browser.searchForServices(ofType: "_googlecast._tcp.", inDomain: "local.")
        deadline = Task { [weak self] in
            do { try await Task.sleep(nanoseconds: 3_000_000_000) } catch { return }
            self?.finish(.success(self?.records.sorted { $0.key < $1.key }.map(\.value).joined(separator: "\n") ?? ""))
        }
    }

    func cancel() {
        finish(.failure(NSError(domain: "Casting.Discovery", code: NSUserCancelledError)))
    }

    private func finish(_ result: Result<String, Error>) {
        deadline?.cancel()
        deadline = nil
        browser.stop()
        browser.delegate = nil
        services.forEach { $0.stop(); $0.delegate = nil }
        services.removeAll()
        let callback = completion
        completion = nil
        callback?(result)
    }

    func netServiceBrowser(_ browser: NetServiceBrowser, didFind service: NetService, moreComing: Bool) {
        guard browser === self.browser, completion != nil else { return }
        services.append(service)
        service.delegate = self
        service.resolve(withTimeout: 2)
    }

    func netServiceBrowser(_ browser: NetServiceBrowser, didRemove service: NetService, moreComing: Bool) {
        guard browser === self.browser, completion != nil else { return }
        records.removeValue(forKey: service.name)
        service.stop()
    }

    func netServiceBrowser(_ browser: NetServiceBrowser, didNotSearch errorDict: [String: NSNumber]) {
        finish(.failure(NSError(domain: "Casting.Discovery", code: errorDict[NetService.errorCode]?.intValue ?? -1,
                                userInfo: [NSLocalizedDescriptionKey: "Local network discovery failed; check local network permission."])))
    }

    func netServiceDidResolveAddress(_ sender: NetService) {
        guard completion != nil, services.contains(sender), let host = sender.hostName, sender.port > 0 else { return }
        let txt = sender.txtRecordData().map(NetService.dictionary(fromTXTRecord:)) ?? [:]
        let name = txt["fn"].flatMap { String(data: $0, encoding: .utf8) } ?? sender.name
        let key = "cast-\(services.firstIndex(of: sender) ?? 0)"
        records[sender.name] = Self.transcript(key: key, name: name, host: host, port: sender.port)
    }

    static func transcript(key: String, name: String, host: String, port: Int) -> String {
        let safeName = name.replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "")
            .replacingOccurrences(of: "\n", with: " ").replacingOccurrences(of: "\r", with: " ")
        return "\(key) SRV 0 0 \(port) \(host)\n\(key) TXT \"fn=\(safeName)\""
    }
}
