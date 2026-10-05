import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

struct BankNode: Decodable {
    let tag: String
    let props: [String: String]
    let text: String?
    let children: [BankNode]?

    var nodes: [BankNode] { children ?? [] }
    var id: String { props["id"] ?? "" }
    var classes: Set<String> { Set((props["className"] ?? "").split(separator: " ").map(String.init)) }
    var label: String { ([text ?? ""] + nodes.map(\.label)).filter { !$0.isEmpty }.joined(separator: " ") }
    var accessibleLabel: String {
        if props["aria-hidden"] == "true" || !classes.isDisjoint(with: ["button-icon", "nav-icon", "account-mark", "account-more"]) { return "" }
        return props["aria-label"] ?? ([text ?? ""] + nodes.map(\.accessibleLabel)).filter { !$0.isEmpty }.joined(separator: " ")
    }
    func has(_ name: String) -> Bool { classes.contains(name) }
    func first(_ predicate: @escaping (BankNode) -> Bool) -> BankNode? {
        predicate(self) ? self : nodes.lazy.compactMap { $0.first(predicate) }.first
    }
    var fields: [BankNode] {
        ["input", "select", "textarea"].contains(tag) ? [self] : nodes.flatMap(\.fields)
    }
    var initialValue: String {
        props["value"] ?? props["defaultValue"] ?? (tag == "select" ? nodes.first?.props["value"] : nil) ?? ""
    }
}

struct BankCommand: Decodable {
    let kind: String
    let id: String?
    let method: String?
    let url: String?
    let body: String?
}

struct BankEnvelope: Decodable {
    let model: String
    let view: BankNode
    let commands: [BankCommand]
}

enum BankFailure: LocalizedError {
    case invalid(String)
    var errorDescription: String? { if case let .invalid(message) = self { return message }; return nil }
}

enum BankProtocol {
    static func json(_ value: [String: Any]) throws -> String {
        let bytes = try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
        guard let result = String(data: bytes, encoding: .utf8) else { throw BankFailure.invalid("Invalid UTF-8 JSON") }
        return result
    }

    static func server(_ value: String) throws -> URL {
        guard let url = URL(string: value.trimmingCharacters(in: .whitespacesAndNewlines)),
              ["http", "https"].contains(url.scheme ?? ""), let host = url.host, !host.isEmpty,
              url.user == nil, url.password == nil, url.query == nil, url.fragment == nil,
              url.path.isEmpty || url.path == "/" else {
            throw BankFailure.invalid("Enter a server origin such as http://192.168.1.20:18790")
        }
        return url
    }

    static func request(_ command: BankCommand, server: URL) throws -> URLRequest {
        guard let path = command.url, path.hasPrefix("/api/"), !path.contains(".."),
              let url = URL(string: path, relativeTo: server)?.absoluteURL,
              let method = command.method, ["GET", "POST"].contains(method) else {
            throw BankFailure.invalid("Invalid bank HTTP command")
        }
        var request = URLRequest(url: url, timeoutInterval: 20)
        request.httpMethod = method
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue("ios-\(command.id ?? "http")-\(UUID().uuidString)", forHTTPHeaderField: "X-Osprey-Request-Id")
        if method == "POST" {
            request.httpBody = Data((command.body ?? "").utf8)
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        return request
    }

    static func form(_ form: BankNode, drafts: [String: String]) -> [String: String] {
        Dictionary(form.fields.compactMap { field in
            guard let name = field.props["name"], field.props["disabled"] != "true" else { return nil }
            return (name, drafts[field.id] ?? field.initialValue)
        }, uniquingKeysWith: { _, latest in latest })
    }
}
