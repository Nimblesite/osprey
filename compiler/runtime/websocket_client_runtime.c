#include "http_shared.h"

// Native text-frame client transport [BUILTIN-WEBSOCKET-CLIENT]
// (docs/specs/0015-WebSockets.md). The language surface exposes no receive
// callback, so incoming client frames are never surfaced.

static bool valid_websocket_id(int64_t ws_id) {
    return ws_id >= 0 && ws_id < MAX_WEBSOCKETS;
}

// TCP connect to host:port. Returns the connected socket, or -1 with *error
// set to the transport's failure code; the socket is closed on every failure.
static int ws_dial(const char *host, int port, int64_t *error) {
    int sock = socket(AF_INET, SOCK_STREAM, 0);
    if (sock < 0) {
        *error = -4;
        return -1;
    }
    struct hostent *server = gethostbyname(host);
    if (!server) {
        close(sock);
        *error = -5;
        return -1;
    }
    struct sockaddr_in server_addr;
    memset(&server_addr, 0, sizeof(server_addr));
    server_addr.sin_family = AF_INET;
    server_addr.sin_port = htons((uint16_t)port);
    memcpy(&server_addr.sin_addr.s_addr, server->h_addr,
           (size_t)server->h_length);
    if (connect(sock, (struct sockaddr *)&server_addr, sizeof(server_addr)) <
        0) {
        close(sock);
        *error = -6;
        return -1;
    }
    return sock;
}

// Sends the RFC 6455 upgrade request and consumes the response. Returns 0, -7
// when the request cannot be sent, or -8 when no response arrives. The
// response is not validated: this transport has no receive surface to use it.
static int64_t ws_upgrade(int sock, const char *host, int port,
                          const char *path) {
    char *ws_key = generate_websocket_key();
    if (!ws_key) {
        return -7;
    }
    char handshake[1024];
    int len = snprintf(handshake, sizeof(handshake),
                       "GET %s HTTP/1.1\r\n"
                       "Host: %s:%d\r\n"
                       "Upgrade: websocket\r\n"
                       "Connection: Upgrade\r\n"
                       "Sec-WebSocket-Key: %s\r\n"
                       "Sec-WebSocket-Version: 13\r\n"
                       "\r\n",
                       path, host, port, ws_key);
    free(ws_key);
    if (len < 0 || (size_t)len >= sizeof(handshake)) {
        return -7;
    }
    if (send(sock, handshake, (size_t)len, 0) < 0) {
        return -7;
    }
    char response[1024];
    if (recv(sock, response, sizeof(response) - 1, 0) <= 0) {
        return -8;
    }
    return 0;
}

// Publishes the connected socket as a handle. Returns the handle, -10 when the
// handle table is exhausted, or -9 when the record cannot be allocated.
static int64_t ws_register(const char *url, int sock) {
    int64_t id = get_next_id();
    if (!valid_websocket_id(id)) {
        return -10;
    }
    WebSocket *ws = malloc(sizeof(WebSocket));
    if (!ws) {
        return -9;
    }
    ws->id = id;
    ws->url = strdup(url);
    ws->socket_fd = sock;
    ws->is_connected = true;
    pthread_mutex_init(&ws->mutex, NULL);

    pthread_mutex_lock(&runtime_mutex);
    websockets[id] = ws;
    pthread_mutex_unlock(&runtime_mutex);
    return id;
}

// Connect to a WebSocket server - returns websocket_id or negative error.
// Only ws:// is accepted; wss:// is rejected [BUILTIN-WEBSOCKET-CLIENT].
int64_t websocket_connect(char *url) {
    if (!url) {
        return -1;
    }
    if (strncmp(url, "ws://", 5) != 0) {
        return -2;
    }
    char *host = NULL;
    char *path = NULL;
    int port = 0;
    if (parse_url(url, &host, &port, &path) != 0) {
        return -3;
    }
    int64_t result = 0;
    int sock = ws_dial(host, port, &result);
    if (sock >= 0) {
        result = ws_upgrade(sock, host, port, path);
        if (result == 0) {
            result = ws_register(url, sock);
        }
        if (result < 0) {
            close(sock);
        }
    }
    free(host);
    free(path);
    return result;
}

// Send message through WebSocket - returns 0 on success or negative error
// [BUILTIN-WEBSOCKET-CLIENT].
int64_t websocket_send(int64_t ws_id, char *message) {
    if (!message) {
        return -1;
    }
    if (!valid_websocket_id(ws_id)) {
        return -2;
    }

    pthread_mutex_lock(&runtime_mutex);
    WebSocket *ws = websockets[ws_id];
    pthread_mutex_unlock(&runtime_mutex);

    if (!ws || !ws->is_connected) {
        return -2;
    }

    pthread_mutex_lock(&ws->mutex);
    int result = send_websocket_frame(ws->socket_fd, message);
    pthread_mutex_unlock(&ws->mutex);

    return result > 0 ? 0 : -3;
}

// Close WebSocket connection - returns 0 on success. Closing an empty slot is
// a no-op, so a double close cannot double-free [BUILTIN-WEBSOCKET-CLIENT].
int64_t websocket_close(int64_t ws_id) {
    if (!valid_websocket_id(ws_id)) {
        return -1;
    }
    pthread_mutex_lock(&runtime_mutex);
    WebSocket *ws = websockets[ws_id];
    if (ws) {
        websockets[ws_id] = NULL;
        ws->is_connected = false;

        if (ws->socket_fd >= 0) {
            close(ws->socket_fd);
        }

        free(ws->url);
        pthread_mutex_destroy(&ws->mutex);
        free(ws);
    }
    pthread_mutex_unlock(&runtime_mutex);

    return 0;
}
