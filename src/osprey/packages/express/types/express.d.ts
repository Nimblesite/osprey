// For the OPTIONAL `tsc` lint only ([FLAVOR-TS-LINT]); the compiler never reads
// it. What `tsc` sees for `import express from "express"`: the methods a
// program may call, each the port function of the same name.
// [TS-PORT-EXPRESS-SURFACE]

/** What a handler passes to `req.fail`, and what an error handler receives. */
export interface HttpError {
  readonly status: int;
  readonly message: string;
}

/** An incoming request: the matched route's captures and the raw wire strings. */
export interface Request {
  readonly method: string;
  readonly url: string;
  readonly path: string;
  readonly params: Map<string, string>;
  readonly query: Map<string, string>;
  readonly headers: string;
  readonly body: string;
  /** `req.get(name)` is the value of the request header `name`, matched without regard to case. */
  get(name: string): Result<string>;
  /** A handler returns `req.next()` to pass the request to the next matching handler. */
  next(): Response;
  /** A handler returns `req.fail(error)` to pass `error` to the error handlers, as `next(error)` does in Express. */
  fail(error: HttpError): Response;
}

/** The response a handler builds; each method returns the extended response. */
export interface Response {
  readonly statusCode: int;
  readonly body: string;
  readonly contentType: string;
  readonly headers: string;
  status(code: int): Response;
  set(name: string, value: string): Response;
  type(mediaType: string): Response;
  send(body: string): Response;
  json(value: Json): Response;
}

/** A handler returns its response, or builds it with statements and returns nothing. */
export type Handler = (req: Request, res: Response) => Response | void;

/** An error handler: Express's four-parameter middleware, without `next`. */
export type ErrorHandler = (error: HttpError, req: Request, res: Response) => Response | void;

/** What `app.listen` returns: a listener and its handle, or why it could not start. */
export type Server =
  | { readonly kind: "listening"; readonly handle: int }
  | { readonly kind: "failed"; readonly message: string };

/** An application: each registration returns the extended application. */
export interface Application {
  get(path: string, handler: Handler): Application;
  post(path: string, handler: Handler): Application;
  put(path: string, handler: Handler): Application;
  delete(path: string, handler: Handler): Application;
  use(path: string, handler: Handler): Application;
  onError(handler: ErrorHandler): Application;
  /** `app.mount(path, router)`: Express spells it `app.use(path, router)`. */
  mount(path: string, router: Application): Application;
  listen(port: int, callback: () => void): Server;
}

/** `express()` creates an application with no routes; `express.Router()` creates a router to mount. */
declare function express(): Application;
declare namespace express {
  function Router(): Application;
}
export default express;

/** `jsonBody(req)`: the request's JSON document, `JsonNull` when it is not JSON, or `malformed JSON`. */
export function jsonBody(req: Request): Result<Json>;

/** `close(server)` stops a listening server; a `failed` one answers why it never listened. */
export function close(server: Server): Result<int>;
