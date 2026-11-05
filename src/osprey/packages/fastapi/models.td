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
type Need { name: String use_cache: Bool }
type Dependency<P, C> { name: String inputs: List<Need> provide: P close: C }
type Acquired<T> { name: String value: T }
type DependencyState<T> { cache: Map<String, T> acquired: List<Acquired<T>> }
union DependencyResult<T> { Provided { value: T } Rejected { error: HTTPException } DependencyInvalid { issues: List<ValidationIssue> } }
type DependencyStep<T, S> { outcome: DependencyResult<T> state: S }
type Application<R, W> { routes: R middleware: W }
union FixtureValue { SessionValue { id: Int } IdentityValue { id: Int role: String } CreditValue { id: Int role: String credit: Int } NumberValue { number: Int } }
type CorsPolicy { origins: List<String> methods: List<String> headers: List<String> expose: List<String> credentials: Bool max_age: Int }
