typeDiagram
type Request { method: String url: String headers: String body: String }
type Response { status_code: Int content: String media_type: String headers: String }
type Route<T> { method: String path: String handler: T }
