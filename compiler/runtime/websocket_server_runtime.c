#include "http_shared.h"
#include "memory_hooks.h"
#include <signal.h>

// Global variable for runtime lifecycle
static volatile int keep_runtime_running = 1;

// Signal handler for graceful shutdown
void handle_shutdown_signal(int sig) {
  printf("\n🛑 Shutdown signal (%d) received, stopping servers...\n", sig);
  keep_runtime_running = 0;
}

// Server connection handling thread data
typedef struct {
  WebSocketServer *server;
  int client_fd;
  struct sockaddr_in client_addr;
} ConnectionData;

// Handle WebSocket client connection in separate thread
void *handle_websocket_connection(void *arg) {
  ConnectionData *data = (ConnectionData *)arg;
  WebSocketServer *server = data->server;
  int client_fd = data->client_fd;

  char buffer[MAX_HTTP_BUFFER];
  int bytes_received = recv(client_fd, buffer, sizeof(buffer) - 1, 0);

  if (bytes_received <= 0) {
    close(client_fd);
    free(data);
    return NULL;
  }

  buffer[bytes_received] = '\0';

  // Check if it's a WebSocket handshake
  if (strstr(buffer, "Upgrade: websocket") &&
      strstr(buffer, "Connection: Upgrade")) {
    // Extract WebSocket key
    char *key_line = strstr(buffer, "Sec-WebSocket-Key: ");
    if (key_line) {
      key_line += strlen("Sec-WebSocket-Key: ");
      char *key_end = strstr(key_line, "\r\n");
      if (key_end) {
        *key_end = '\0';

        // Create handshake response
        char *response = create_websocket_handshake_response(key_line);

        if (response && send(client_fd, response, strlen(response), 0) > 0) {
          // Create WebSocket connection
          int64_t ws_id = get_next_id();
          WebSocket *ws = malloc(sizeof(WebSocket));
          if (ws) {
            ws->id = ws_id;
            ws->url = strdup("server-connection");
            ws->message_handler = strdup("server-handler");
            ws->socket_fd = client_fd;
            ws->is_connected = true;
            pthread_mutex_init(&ws->mutex, NULL);

            // Add to server connections
            pthread_mutex_lock(&server->mutex);
            if (server->connection_count < MAX_CONNECTIONS_PER_SERVER) {
              server->connections[server->connection_count++] = ws;
              pthread_mutex_unlock(&server->mutex);

              // Send welcome message
              char welcome[512];
              snprintf(welcome, sizeof(welcome),
                       "{\"type\":\"welcome\",\"message\":\"Connected to "
                       "Osprey WebSocket Server!\",\"timestamp\":\"%lld\"}",
                       (long long)time(NULL));

              send_websocket_frame(client_fd, welcome);

              // Handle incoming messages
              char frame_buffer[MAX_HTTP_BUFFER];
              while (ws->is_connected) {
                int frame_bytes =
                    recv(client_fd, frame_buffer, sizeof(frame_buffer), 0);
                if (frame_bytes <= 0)
                  break;

                char *payload = NULL;
                int payload_len =
                    parse_websocket_frame(frame_buffer, frame_bytes, &payload);

                if (payload_len > 0 && payload) {
                  printf("WebSocket received: %s\n", payload);

                  // Echo message back with server info
                  char echo_response[1024];
                  snprintf(echo_response, sizeof(echo_response),
                           "{\"type\":\"echo\",\"original\":\"%s\",\"echo\":"
                           "\"Server received: %s\",\"timestamp\":\"%lld\"}",
                           payload, payload, (long long)time(NULL));

                  send_websocket_frame(client_fd, echo_response);
                  free(payload);
                }
              }
            } else {
              pthread_mutex_unlock(&server->mutex);
              free(ws);
            }
          }
        }

        if (response)
          free(response);
      }
    }
  }

  close(client_fd);
  free(data);
  return NULL;
}

// Server listening thread
void *websocket_server_thread(void *arg) {
  WebSocketServer *server = (WebSocketServer *)arg;

  struct sockaddr_in client_addr;
  socklen_t client_len = sizeof(client_addr);

  while (server->is_listening) {
    int client_fd =
        accept(server->socket_fd, (struct sockaddr *)&client_addr, &client_len);
    if (client_fd < 0) {
      if (server->is_listening) {
        printf("WebSocket server accept error: %s\n", strerror(errno));
      }
      continue;
    }

    printf("WebSocket server: New connection from %s\n",
           inet_ntoa(client_addr.sin_addr));

    // Create connection data and spawn thread
    ConnectionData *data = malloc(sizeof(ConnectionData));
    data->server = server;
    data->client_fd = client_fd;
    data->client_addr = client_addr;

    pthread_t connection_thread;
    // The handler thread runs user callbacks that allocate on the shared heap
    // concurrently with this accept loop — lock the memory backend first.
    osp_mem_notify_multithreaded();
    if (pthread_create(&connection_thread, NULL, handle_websocket_connection,
                       data) != 0) {
      printf("Failed to create connection thread\n");
      close(client_fd);
      free(data);
    } else {
      pthread_detach(connection_thread);
    }
  }

  return NULL;
}

// Create WebSocket server - returns server_id or negative error
int64_t websocket_create_server(int64_t port, char *address, char *path) {
  if (port < 1 || port > 65535) {
    return -1;
  }

  if (!address || !path) {
    return -2;
  }

  int64_t id = get_next_id();
  WebSocketServer *server = malloc(sizeof(WebSocketServer));
  if (!server) {
    return -3;
  }

  server->id = id;
  server->port = (int)port;
  server->address = strdup(address);
  server->path = strdup(path);
  server->socket_fd = -1;
  server->is_listening = false;
  server->connection_count = 0;
  pthread_mutex_init(&server->mutex, NULL);

  // Initialize connections array
  for (int i = 0; i < MAX_CONNECTIONS_PER_SERVER; i++) {
    server->connections[i] = NULL;
  }

  pthread_mutex_lock(&runtime_mutex);
  websocket_servers[id] = server;
  pthread_mutex_unlock(&runtime_mutex);

  return id;
}

// Start WebSocket server listening - returns 0 on success
int64_t websocket_server_listen(int64_t server_id) {
  pthread_mutex_lock(&runtime_mutex);
  WebSocketServer *server = websocket_servers[server_id];
  pthread_mutex_unlock(&runtime_mutex);

  if (!server) {
    return -1;
  }

  // Create socket
  server->socket_fd = socket(AF_INET, SOCK_STREAM, 0);
  if (server->socket_fd < 0) {
    return -2;
  }

  // Set socket options
  int opt = 1;
  // optval is `const char *` on Winsock, `const void *` on POSIX. [WINDOWS-PORT-PHASE2]
  if (setsockopt(server->socket_fd, SOL_SOCKET, SO_REUSEADDR, (const char *)&opt,
                 sizeof(opt)) < 0) {
    close(server->socket_fd);
    return -3;
  }

  // Bind socket
  struct sockaddr_in server_addr;
  server_addr.sin_family = AF_INET;
  server_addr.sin_port = htons(server->port);
  server_addr.sin_addr.s_addr = inet_addr(server->address);

  if (bind(server->socket_fd, (struct sockaddr *)&server_addr,
           sizeof(server_addr)) < 0) {
    close(server->socket_fd);
    return -4;
  }

  // Start listening
  if (listen(server->socket_fd, SOMAXCONN) < 0) {
    close(server->socket_fd);
    return -5;
  }

  server->is_listening = true;

  // The server thread accepts and dispatches concurrently with the caller.
  osp_mem_notify_multithreaded();
  // Start server thread
  if (pthread_create(&server->server_thread, NULL, websocket_server_thread,
                     server) != 0) {
    close(server->socket_fd);
    server->is_listening = false;
    return -6;
  }

  return 0;
}

// Send message to specific WebSocket connection - returns 0 on success
int64_t websocket_server_send(int64_t server_id, int64_t connection_id,
                              char *message) {
  if (!message) {
    return -1;
  }

  pthread_mutex_lock(&runtime_mutex);
  WebSocketServer *server = websocket_servers[server_id];
  pthread_mutex_unlock(&runtime_mutex);

  if (!server) {
    return -2;
  }

  pthread_mutex_lock(&server->mutex);
  for (int i = 0; i < server->connection_count; i++) {
    WebSocket *ws = server->connections[i];
    if (ws && ws->id == connection_id && ws->is_connected) {
      int result = send_websocket_frame(ws->socket_fd, message);
      pthread_mutex_unlock(&server->mutex);
      return result > 0 ? 0 : -3;
    }
  }
  pthread_mutex_unlock(&server->mutex);

  return -4; // Connection not found
}

// Broadcast message to all connections - returns number of connections sent to
int64_t websocket_server_broadcast(int64_t server_id, char *message) {
  if (!message) {
    return -1;
  }

  pthread_mutex_lock(&runtime_mutex);
  WebSocketServer *server = websocket_servers[server_id];
  pthread_mutex_unlock(&runtime_mutex);

  if (!server) {
    return -2;
  }

  int sent_count = 0;
  pthread_mutex_lock(&server->mutex);
  for (int i = 0; i < server->connection_count; i++) {
    WebSocket *ws = server->connections[i];
    if (ws && ws->is_connected) {
      if (send_websocket_frame(ws->socket_fd, message) > 0) {
        sent_count++;
      }
    }
  }
  pthread_mutex_unlock(&server->mutex);

  return sent_count;
}

// Stop WebSocket server - returns 0 on success
int64_t websocket_stop_server(int64_t server_id) {
  pthread_mutex_lock(&runtime_mutex);
  WebSocketServer *server = websocket_servers[server_id];
  if (server) {
    websocket_servers[server_id] = NULL;
    server->is_listening = false;

    // Close all connections
    pthread_mutex_lock(&server->mutex);
    for (int i = 0; i < server->connection_count; i++) {
      WebSocket *ws = server->connections[i];
      if (ws) {
        ws->is_connected = false;
        close(ws->socket_fd);
        free(ws->url);
        free(ws->message_handler);
        pthread_mutex_destroy(&ws->mutex);
        free(ws);
      }
    }
    pthread_mutex_unlock(&server->mutex);

    // Close server socket
    if (server->socket_fd >= 0) {
      close(server->socket_fd);
    }

    // Wait for server thread to finish
    pthread_join(server->server_thread, NULL);

    free(server->address);
    free(server->path);
    pthread_mutex_destroy(&server->mutex);
    free(server);
  }
  pthread_mutex_unlock(&runtime_mutex);

  return 0;
}

// Keep WebSocket server alive - blocks until interrupted
void websocket_keep_alive() {
  printf("🔄 WebSocket server running - Press Ctrl+C to stop\n");

  // Set up signal handling for graceful shutdown
  signal(SIGINT, handle_shutdown_signal);
  signal(SIGTERM, handle_shutdown_signal);

  // Keep running until shutdown signal received
  while (keep_runtime_running) {
    sleep(1);
  }

  printf("🛑 Shutting down all WebSocket servers...\n");

  // Stop all servers
  pthread_mutex_lock(&runtime_mutex);
  for (int i = 0; i < MAX_WEBSOCKET_SERVERS; i++) {
    if (websocket_servers[i]) {
      websocket_stop_server(i);
    }
  }
  pthread_mutex_unlock(&runtime_mutex);
}