typeDiagram
type Request { method: String url: String headers: String body: String path_params: Map<String, String> }
type Response { status_code: Int content: String media_type: String headers: String }
type Route<T> { method: String path: String handler: T }
type Decoded { value: String rest: String }
type Parameter { name: String value: String }
type HTTPException { status_code: Int detail: String headers: String }
type ValidationIssue { location: List<String> kind: String message: String input: String missing: Bool context: ValidationContext }
union Validation<T> { Valid { value: T } Invalid { issues: List<ValidationIssue> } }
type Pair<A, B> { first: A second: B }
union ValidationContext { NoContext {} TextContext { name: String value: String } NumberContext { name: String value: Int } }
