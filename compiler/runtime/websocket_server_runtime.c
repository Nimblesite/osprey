#include "http_shared.h"
#include "socket_safety.h"
#include "memory_hooks.h"
#include <signal.h>

#ifdef _WIN32
#define WS_SHUTDOWN_BOTH SD_BOTH
#else
#define WS_SHUTDOWN_BOTH SHUT_RDWR
#endif

// Native text-frame server transport [BUILTIN-WEBSOCKET-SERVER]
// (docs/specs/runtime/0015-WebSockets.md). Lifecycle: [BUILTIN-WEBSOCKET-LIFECYCLE].

// Global variable for runtime lifecycle
static volatile int keep_runtime_running = 1;

static bool valid_websocket_server_id(int64_t server_id) {
  return server_id >= 0 && server_id < MAX_WEBSOCKET_SERVERS;
}

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

// How often a handler looks up from a silent peer to see if the server is stopping,
// and how long a send can stall on a peer that stopped reading before it fails.
enum { WS_TICK_MS = 1000 };

// recv, repeated while the peer is merely quiet: <= 0 when it left or the server stops.
static int recv_until_stopped(const WebSocketServer *server, int fd, char *buffer, size_t size) {
  for (;;) {
    int received = (int)recv(fd, buffer, size, 0);
    int error = osp_socket_error();
    bool quiet = received < 0 && (osp_socket_timed_out(error) || error == EINTR);
    if (!quiet || !atomic_load(&server->is_listening)) {
      return received;
    }
  }
}

// Lists an upgraded connection, for broadcasts and for stop to find. NULL when the
// server is full or out of memory, or has begun stopping and won't see it.
static WebSocket *register_connection(WebSocketServer *server, int fd) {
  WebSocket *ws = calloc(1, sizeof(WebSocket));
  if (!ws) {
    return NULL;
  }
  ws->socket_fd = fd;
  ws->is_connected = true;
  pthread_mutex_lock(&server->mutex);
  bool room = server->connection_count < MAX_CONNECTIONS_PER_SERVER &&
              atomic_load(&server->is_listening);
  if (room) {
    server->connections[server->connection_count++] = ws;
  }
  pthread_mutex_unlock(&server->mutex);
  if (!room) {
    free(ws);
    return NULL;
  }
  return ws;
}

// Unlists it before its descriptor can close, so no broadcast can write to whatever
// socket reuses that number.
static void unregister_connection(WebSocketServer *server, WebSocket *ws) {
  pthread_mutex_lock(&server->mutex);
  for (int i = 0; i < server->connection_count; i++) {
    if (server->connections[i] == ws) {
      server->connections[i] = server->connections[--server->connection_count];
      server->connections[server->connection_count] = NULL;
      break;
    }
  }
  pthread_mutex_unlock(&server->mutex);
  free(ws);
}

static void send_welcome(int fd) {
  char welcome[512];
  snprintf(welcome, sizeof(welcome),
           "{\"type\":\"welcome\",\"message\":\"Connected to "
           "Osprey WebSocket Server!\",\"timestamp\":\"%lld\"}",
           (long long)time(NULL));
  send_websocket_frame(fd, welcome);
}

// Echoes each message back with server info until the peer goes or the server stops.
static void serve_messages(const WebSocketServer *server, int fd) {
  char frame_buffer[MAX_HTTP_BUFFER];
  for (;;) {
    int frame_bytes = recv_until_stopped(server, fd, frame_buffer, sizeof(frame_buffer));
    if (frame_bytes <= 0) {
      return;
    }
    char *payload = NULL;
    int payload_len = parse_websocket_frame(frame_buffer, frame_bytes, &payload);
    if (payload_len > 0 && payload) {
      fprintf(stderr, "WebSocket received: %s\n", payload);
      char echo_response[1024];
      snprintf(echo_response, sizeof(echo_response),
               "{\"type\":\"echo\",\"original\":\"%s\",\"echo\":"
               "\"Server received: %s\",\"timestamp\":\"%lld\"}",
               payload, payload, (long long)time(NULL));
      send_websocket_frame(fd, echo_response);
    }
    free(payload); // Empty frames also own a parser allocation [BUILTIN-WEBSOCKET-FRAMING].
  }
}

// Sends the handshake's answer and serves the connection while it lives.
static void serve_upgrade(WebSocketServer *server, int fd, char *request) {
  if (!strstr(request, "Upgrade: websocket") || !strstr(request, "Connection: Upgrade")) {
    return;
  }
  char *key_line = strstr(request, "Sec-WebSocket-Key: ");
  char *key_end = key_line ? strstr(key_line, "\r\n") : NULL;
  if (!key_end) {
    return;
  }
  *key_end = '\0';
  char *response = create_websocket_handshake_response(key_line + strlen("Sec-WebSocket-Key: "));
  if (response && osp_socket_send(fd, response, strlen(response)) > 0) {
    send_welcome(fd); // first, so no broadcast can reach the peer ahead of it
    WebSocket *ws = register_connection(server, fd);
    if (ws) {
      serve_messages(server, fd);
      unregister_connection(server, ws);
    }
  }
  free(response);
}

// Handle WebSocket client connection in separate thread. Its last act says it has
// finished: stop frees the server as soon as the last one has.
void *handle_websocket_connection(void *arg) {
  ConnectionData *data = (ConnectionData *)arg;
  WebSocketServer *server = data->server;
  int client_fd = data->client_fd;
  free(data);
  osp_socket_deadline(client_fd, WS_TICK_MS);
  char buffer[MAX_HTTP_BUFFER];
  int bytes_received = recv_until_stopped(server, client_fd, buffer, sizeof(buffer) - 1);
  if (bytes_received > 0) {
    buffer[bytes_received] = '\0';
    serve_upgrade(server, client_fd, buffer);
  }
  close(client_fd);
  pthread_mutex_lock(&server->mutex);
  server->handler_count--;
  pthread_cond_broadcast(&server->handlers_gone);
  pthread_mutex_unlock(&server->mutex);
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
        fprintf(stderr, "WebSocket server accept error: %s\n", strerror(errno));
      }
      continue;
    }

    // The program's stdout is the program's: logs go to stderr, as the HTTP server's do.
    fprintf(stderr, "WebSocket server: New connection from %s\n",
            inet_ntoa(client_addr.sin_addr));

    // Create connection data and spawn thread
    ConnectionData *data = malloc(sizeof(ConnectionData));
    if (!data) {
      close(client_fd);
      continue;
    }
    (void)osp_socket_configure_no_sigpipe(client_fd);
    data->server = server;
    data->client_fd = client_fd;
    data->client_addr = client_addr;

    pthread_t connection_thread;
    // The handler thread runs user callbacks that allocate on the shared heap
    // concurrently with this accept loop — lock the memory backend first.
    osp_mem_notify_multithreaded();
    pthread_mutex_lock(&server->mutex);
    server->handler_count++;
    pthread_mutex_unlock(&server->mutex);
    if (pthread_create(&connection_thread, NULL, handle_websocket_connection,
                       data) != 0) {
      fprintf(stderr, "Failed to create connection thread\n");
      close(client_fd);
      free(data);
      pthread_mutex_lock(&server->mutex);
      server->handler_count--;
      pthread_mutex_unlock(&server->mutex);
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
  if (!valid_websocket_server_id(id) || id == 0) {
    return -3;
  }
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
  server->thread_started = false;
  server->connection_count = 0;
  server->handler_count = 0;
  pthread_mutex_init(&server->mutex, NULL);
  pthread_cond_init(&server->handlers_gone, NULL);

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
  if (!valid_websocket_server_id(server_id)) {
    return -1;
  }
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
  // optval is `const char *` on Winsock, `const void *` on POSIX.
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
  server->thread_started = true;

  return 0;
}

// Broadcast message to all connections - returns number of connections sent to
int64_t websocket_server_broadcast(int64_t server_id, char *message) {
  if (!message) {
    return -1;
  }
  if (!valid_websocket_server_id(server_id)) {
    return -2;
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
      } else {
        // It missed a frame, or got part of one, so its stream can't go on. Its
        // handler wakes to this and unlists it.
        ws->is_connected = false;
        (void)shutdown(ws->socket_fd, WS_SHUTDOWN_BOTH);
      }
    }
  }
  pthread_mutex_unlock(&server->mutex);

  return sent_count;
}

// Ends every connection and waits for the threads serving them, which read the server
// to their last. The accept thread has gone, so no more can start. A handler still
// shaking hands isn't listed, and notices the stop within a tick.
static void end_connections(WebSocketServer *server) {
  pthread_mutex_lock(&server->mutex);
  for (int i = 0; i < server->connection_count; i++) {
    WebSocket *ws = server->connections[i];
    ws->is_connected = false;
    (void)shutdown(ws->socket_fd, WS_SHUTDOWN_BOTH);
  }
  while (server->handler_count > 0) {
    pthread_cond_wait(&server->handlers_gone, &server->mutex);
  }
  pthread_mutex_unlock(&server->mutex);
}

// Stop WebSocket server - returns 0 on success
int64_t websocket_stop_server(int64_t server_id) {
  if (!valid_websocket_server_id(server_id)) {
    return -1;
  }
  pthread_mutex_lock(&runtime_mutex);
  WebSocketServer *server = websocket_servers[server_id];
  websocket_servers[server_id] = NULL;
  pthread_mutex_unlock(&runtime_mutex);
  if (!server) {
    return 0;
  }
  atomic_store(&server->is_listening, false);
  // SHUT DOWN before closing, or the join below never returns. A `close()`
  // does NOT interrupt a thread already blocked in `accept()` on Linux — the
  // descriptor goes away and the thread stays parked on it forever. macOS
  // does wake it, which is why this suite passed here and hung for the whole
  // 60-minute CI budget on the runner, killed as an orphan process with no
  // failing assertion to name. `shutdown` wakes a blocked `accept()` on both,
  // and `is_listening` is already false above, so the loop ends rather than
  // spinning on the error.
  if (server->socket_fd >= 0) {
    (void)shutdown(server->socket_fd, WS_SHUTDOWN_BOTH);
    close(server->socket_fd);
  }
  if (server->thread_started) {
    pthread_join(server->server_thread, NULL);
  }
  end_connections(server);
  free(server->address);
  free(server->path);
  pthread_cond_destroy(&server->handlers_gone);
  pthread_mutex_destroy(&server->mutex);
  free(server);
  return 0;
}

// Keep WebSocket server alive - blocks until interrupted
void websocket_keep_alive(void) {
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
  for (int i = 0; i < MAX_WEBSOCKET_SERVERS; i++) {
    websocket_stop_server(i);
  }
}
