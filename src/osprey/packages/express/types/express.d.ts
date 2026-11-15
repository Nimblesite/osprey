// For the OPTIONAL `tsc` lint only ([FLAVOR-TS-LINT]); the compiler never reads
// it. What `tsc` sees for `import express from "express"`: the methods a
// program may call, each the port function of the same name.
// [TS-PORT-EXPRESS-SURFACE]

/** An incoming request: the matched route's captures and the raw wire strings. */
export interface Request {
  readonly method: string;
  readonly url: string;
  readonly path: string;
  readonly params: Map<string, string>;
  readonly headers: string;
  readonly body: string;
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
}

/** A handler returns its response, or builds it with statements and returns nothing. */
export type Handler = (req: Request, res: Response) => Response | void;

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
  listen(port: int, callback: () => void): Server;
}

/** `express()` creates an application with no routes. */
export default function express(): Application;

/** `close(server)` stops a listening server; a `failed` one answers why it never listened. */
export function close(server: Server): Result<int>;
