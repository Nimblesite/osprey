// What the WebSocket test TUs share: the server API and a raw loopback client.
#ifndef WEBSOCKET_TEST_HELPERS_H
#define WEBSOCKET_TEST_HELPERS_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

int64_t websocket_create_server(int64_t port, char *address, char *path);
int64_t websocket_server_listen(int64_t server_id);
int64_t websocket_server_broadcast(int64_t server_id, char *message);
int64_t websocket_stop_server(int64_t server_id);

int ws_raw_connect(int port);
void ws_read_headers(int fd, char *out, size_t capacity);
char *ws_read_text_frame(int fd);
char *ws_read_frame(int fd, unsigned char *opcode, size_t *length);
void ws_send_all(int fd, const void *buf, size_t len);
bool ws_read_exact(int fd, void *buf, size_t len);
void ws_send_upgrade(int fd, const char *key_header);
void ws_expect_peer_close(int fd);

void test_websocket_lifecycle(void); // websocket_lifecycle_tests.c
void test_websocket_frames(void);    // websocket_frames_tests.c

#endif // WEBSOCKET_TEST_HELPERS_H
