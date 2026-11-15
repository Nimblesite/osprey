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
  listen(port: int, callback: () => void): Server;
}

/** `express()` creates an application with no routes. */
export default function express(): Application;

/** `close(server)` stops a listening server; a `failed` one answers why it never listened. */
export function close(server: Server): Result<int>;
