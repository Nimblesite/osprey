#ifndef HTTP_SERVER_POOL_STUB_H
#define HTTP_SERVER_POOL_STUB_H

// The pool drives real sockets and fibers. A unit that includes
// http_server_runtime.c to reach its statics serves no connection through the
// pool, so it links these in its place.
void http_pool_run(HttpServer *pooled) { (void)pooled; }
bool http_pool_owns_thread(HttpServer *pooled) {
  (void)pooled;
  return false;
}
void http_pool_shutdown_clients(HttpServer *pooled) { (void)pooled; }
size_t http_pool_serving(HttpServer *pooled) {
  (void)pooled;
  return 0;
}

#endif
