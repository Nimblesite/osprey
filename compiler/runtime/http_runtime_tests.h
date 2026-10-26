#ifndef HTTP_RUNTIME_TESTS_H
#define HTTP_RUNTIME_TESTS_H

#include "http_shared.h"

// Shared declarations for the HTTP runtime suite TUs (http_runtime_tests.c,
// http_origin_tests.c, http_tls_tests.c, http_server_live_tests.c), which link
// into one suite binary.

// Runtime APIs under test.
extern int64_t http_create_client(char *base_url, int64_t timeout);
extern int64_t http_close_client(int64_t client_id);
extern int64_t http_get(int64_t client_id, char *path, char *headers);
extern int64_t http_post(int64_t client_id, char *path, char *body,
                         char *headers);
extern int64_t http_put(int64_t client_id, char *path, char *body,
                        char *headers);
extern int64_t http_delete(int64_t client_id, char *path, char *headers);

extern int64_t http_create_server(int64_t port, char *address);
extern int64_t http_listen(int64_t server_id, HttpRequestHandler handler);
extern int64_t http_stop_server(int64_t server_id);

extern int64_t http_get_response(int64_t client_id, char *path, char *headers);
extern int64_t http_post_response(int64_t client_id, char *path, char *body,
                                  char *headers);
extern int64_t http_response_status(int64_t handle);
extern char *http_response_body(int64_t handle);
extern char *http_response_header(int64_t handle, char *name);
extern int64_t http_response_free(int64_t handle);

// Defined in http_server_live_tests.c; the lifecycle test needs a real
// callback now that http_listen is declared with its true signature.
HttpResponse *http_test_server_handler(char *method, char *path,
                                       char *headers, char *body);

// Test entry points, invoked in order by run_all_http_tests (main TU).
// The first two drive the allocation injector that lives in the main TU.
void test_alloc_fail_off(void);
void test_alloc_fail_size(size_t bytes);
void test_http_create_client(void);
void test_http_create_server(void);
void test_http_server_lifecycle(void);
void test_rejection_mapping(void);
void test_sanitize_log_token(void);
void test_socket_interrupted(void);
void test_response_handle_rejection(void);
void test_live_loopback_exchange(void);
void test_post_response_loopback(void);
void test_client_handle_bounds(void);
void test_client_handle_exhaustion(void);
void test_client_chunked_response(void);
void test_client_status_line_handling(void);
void test_client_methods_and_body(void);
void test_client_transport_failures(void);
void test_client_large_response(void);
void test_client_malformed_chunks(void);
void test_client_response_allocation_failures(void);
void test_client_tls_handshake_failure(void);
void test_client_tls_peer_verification(void);
void test_server_rejects_bad_requests(void);
void test_server_self_stop_from_handler(void);
void test_server_handle_bounds(void);
void test_server_labels_an_unparseable_request(void);
void test_listener_spawn_failure_is_reported(void);
void test_server_handle_exhaustion(void);

#endif // HTTP_RUNTIME_TESTS_H
