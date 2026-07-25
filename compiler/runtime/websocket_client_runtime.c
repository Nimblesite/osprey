#include "http_shared.h"

// Connect to WebSocket server - returns websocket_id or negative error.
// The language has no receive callback; incoming client frames are not exposed.
// [BUILTIN-WEBSOCKET]
int64_t websocket_connect(char *url) {
    if (!url) {
        return -1;
    }
    if (strncmp(url, "ws://", 5) != 0) {
        return -2;
    }
    
    // Parse WebSocket URL (ws://host:port/path)
    char* host;
    int port;
    char* path;
    
    if (parse_url(url, &host, &port, &path) != 0) {
        return -3;
    }
    
    // Create socket
    int sock = socket(AF_INET, SOCK_STREAM, 0);
    if (sock < 0) {
        free(host);
        free(path);
        return -4;
    }
    
    // Connect to server
    struct sockaddr_in server_addr;
    server_addr.sin_family = AF_INET;
    server_addr.sin_port = htons(port);
    
    struct hostent* server = gethostbyname(host);
    if (!server) {
        close(sock);
        free(host);
        free(path);
        return -5;
    }
    
    memcpy(&server_addr.sin_addr.s_addr, server->h_addr, server->h_length);
    
    if (connect(sock, (struct sockaddr*)&server_addr, sizeof(server_addr)) < 0) {
        close(sock);
        free(host);
        free(path);
        return -6;
    }
    
    // Send WebSocket handshake
    char* ws_key = generate_websocket_key();
    char handshake[1024];
    snprintf(handshake, sizeof(handshake),
        "GET %s HTTP/1.1\r\n"
        "Host: %s:%d\r\n"
        "Upgrade: websocket\r\n"
        "Connection: Upgrade\r\n"
        "Sec-WebSocket-Key: %s\r\n"
        "Sec-WebSocket-Version: 13\r\n"
        "\r\n",
        path, host, port, ws_key);
    
    if (send(sock, handshake, strlen(handshake), 0) < 0) {
        close(sock);
        free(host);
        free(path);
        free(ws_key);
        return -7;
    }
    
    // Read handshake response (simplified)
    char response[1024];
    if (recv(sock, response, sizeof(response) - 1, 0) <= 0) {
        close(sock);
        free(host);
        free(path);
        free(ws_key);
        return -8;
    }
    
    // Create WebSocket structure
    int64_t id = get_next_id();
    WebSocket* ws = malloc(sizeof(WebSocket));
    if (!ws) {
        close(sock);
        free(host);
        free(path);
        free(ws_key);
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
    
    free(host);
    free(path);
    free(ws_key);
    
    return id;
}

// Send message through WebSocket - returns 0 on success or negative error
int64_t websocket_send(int64_t ws_id, char* message) {
    if (!message) {
        return -1;
    }
    
    pthread_mutex_lock(&runtime_mutex);
    WebSocket* ws = websockets[ws_id];
    pthread_mutex_unlock(&runtime_mutex);
    
    if (!ws || !ws->is_connected) {
        return -2;
    }
    
    pthread_mutex_lock(&ws->mutex);
    int result = send_websocket_frame(ws->socket_fd, message);
    pthread_mutex_unlock(&ws->mutex);
    
    return result > 0 ? 0 : -3;
}

// Close WebSocket connection - returns 0 on success
int64_t websocket_close(int64_t ws_id) {
    pthread_mutex_lock(&runtime_mutex);
    WebSocket* ws = websockets[ws_id];
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
