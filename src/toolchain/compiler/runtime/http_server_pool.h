#ifndef HTTP_SERVER_POOL_H
#define HTTP_SERVER_POOL_H

#include "http_server_internal.h"

#ifdef _WIN32
#define HTTP_SHUTDOWN_BOTH SD_BOTH
#else
#define HTTP_SHUTDOWN_BOTH SHUT_RDWR
#endif

// The listener's connections [HTTP-SERVER-CONNECTIONS]: one poller fiber
// watches the listening socket and every idle keep-alive connection, and hands
// each connection with a request waiting to a pool of worker fibers. A
// connection belongs to exactly one of them at a time, and only its owner
// closes it.

// Serve the next request on `client_fd`; true when the connection stays open
// for another one. Defined in http_server_runtime.c.
bool http_serve_connection(HttpServer *server, int client_fd);
bool http_server_is_listening(HttpServer *server);
int http_server_socket(HttpServer *server);

// The poller's whole life: start the workers, serve until the server stops
// listening, then join the workers and close every connection it still owns.
void http_pool_run(HttpServer *server);

// Whether the calling thread is one of `server`'s workers. The caller holds
// server->mutex.
bool http_pool_owns_thread(HttpServer *server);

// Shut down every connection a worker is serving, except the calling thread's
// own: a handler that stops its server still writes its reply. The caller
// holds server->mutex.
void http_pool_shutdown_clients(HttpServer *server);

// Workers read requests and send replies concurrently, but one handler runs
// at a time: what a handler's externs share (a SQLite handle, a C library's
// globals) is not known to be thread-safe. A waiting worker is parked, so the
// collector never waits on it.
void http_pool_enter_handler(HttpServer *server);
void http_pool_leave_handler(HttpServer *server);

// How many connections workers are serving right now.
size_t http_pool_serving(HttpServer *server);

#endif
