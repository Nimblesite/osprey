#include "http_shared.h"
#include <limits.h>

// Forward declaration of fiber functions
extern int64_t fiber_spawn(int64_t (*fn)(void));

// Thread-safe server context structure
typedef struct {
  int64_t server_id;
} ServerContext;

// Simple HTTP response
static const char *simple_response_body = "Hello, World!";
static const char *simple_response_headers = "HTTP/1.1 200 OK\r\n"
                                            "Content-Type: text/plain\r\n"
                                            "Content-Length: %zu\r\n"
                                            "Connection: close\r\n"
                                            "\r\n";

// `send` is allowed to write fewer bytes than requested (and can be interrupted
// after writing none). HTTP Content-Length promises the whole payload, so every
// response write must drain the buffer before the connection is closed.
static bool send_was_interrupted(void) {
#ifdef _WIN32
  return WSAGetLastError() == WSAEINTR;
#else
  return errno == EINTR;
#endif
}

static int send_some(int socket_fd, const char *data, size_t length) {
  int chunk = length > (size_t)INT_MAX ? INT_MAX : (int)length;
  int written = send(socket_fd, data, chunk, 0);
  if (written > 0) {
    return written;
  }
  return written < 0 && send_was_interrupted() ? 0 : -1;
}

static int send_all(int socket_fd, const char *data, size_t length) {
  size_t offset = 0;
  while (offset < length) {
    int written = send_some(socket_fd, data + offset, length - offset);
    if (written < 0) {
      return -1;
    }
    offset += (size_t)written;
  }
  return 0;
}

// Server loop fiber function that actually handles requests
static int64_t server_loop_fiber(void) {
  // Get server from runtime (thread-safe)
  pthread_mutex_lock(&runtime_mutex);
  HttpServer *server = NULL;

  // Find the listening server (there should only be one at a time)
  for (int i = 1; i < MAX_SERVERS; i++) {
    if (servers[i] && servers[i]->is_listening) {
      server = servers[i];
      break;
    }
  }
  pthread_mutex_unlock(&runtime_mutex);

  if (!server) {
    return -1;
  }

  // Keep accepting connections in a loop
  while (server->is_listening) {
    struct sockaddr_in client_addr;
    socklen_t client_len = sizeof(client_addr);

    int client_fd =
        accept(server->socket_fd, (struct sockaddr *)&client_addr, &client_len);
    if (client_fd >= 0) {
      // Read the full HTTP request
      char buffer[4096];
      ssize_t bytes_read = recv(client_fd, buffer, sizeof(buffer) - 1, 0);
      if (bytes_read > 0) {
        buffer[bytes_read] = '\0';

        // Parse the request line to get method and path
        char method[16] = {0};
        char path[256] = {0};
        sscanf(buffer, "%15s %255s", method, path);

        // Dim, recessive trace on stderr so a program's own colored access log
        // (on stdout) is what stands out.
        fprintf(stderr, "\x1b[38;5;238m      · %s %s\x1b[0m\n", method, path);

        // Find the request body (after \r\n\r\n)
        char *body_start = strstr(buffer, "\r\n\r\n");
        char *body = "";
        if (body_start) {
          body = body_start + 4;
        }

        // Call the Osprey handler function directly
        struct HttpResponse *response = NULL;

        // Build raw headers string (simplified for now)
        char raw_headers[1024] = "";

        if (server->handler) {
          response = server->handler(method, path, raw_headers, body);
        }

        // Build and send HTTP response
        char http_response[8192];
        if (response && response->partialBody) {
          // Calculate actual body length instead of using hardcoded partialLength
          size_t actual_body_length = strlen(response->partialBody);

          // Build proper HTTP response with status and headers
          snprintf(http_response, sizeof(http_response),
                   "HTTP/1.1 %" PRId64 " %s\r\n"
                   "%s"
                   "Content-Length: %zu\r\n"
                   "Connection: close\r\n"
                   "\r\n",
                   response->status,
                   (response->status == 200)   ? "OK"
                   : (response->status == 404) ? "Not Found"
                   : (response->status == 405) ? "Method Not Allowed"
                                               : "Error",
                   response->headers ? response->headers : "",
                   actual_body_length);

          // Send headers first, then every body byte. Large generated JS/CSS
          // bundles commonly exceed one socket-buffer write.
          if (send_all(client_fd, http_response, strlen(http_response)) == 0) {
            (void)send_all(client_fd, response->partialBody, actual_body_length);
          }

          // Clean up allocated memory (if needed)
          // Note: Osprey-allocated memory should be managed by Osprey
        } else {
          // Fallback to simple response with dynamic length calculation
          size_t body_len = strlen(simple_response_body);
          snprintf(http_response, sizeof(http_response), simple_response_headers,
                   body_len);
          if (send_all(client_fd, http_response, strlen(http_response)) == 0) {
            (void)send_all(client_fd, simple_response_body, body_len);
          }
        }
      }
      close(client_fd);
    }
  }

  return 0;
}

// Create HTTP server - returns server_id or negative error
int64_t http_create_server(int64_t port, char *address) {
  if (port < 1 || port > 65535) {
    return -1;
  }

  if (!address) {
    return -2;
  }

  int64_t id = get_next_id();
  HttpServer *server = malloc(sizeof(HttpServer));
  if (!server) {
    return -3;
  }

  server->id = id;
  server->port = (int)port;
  server->address = strdup(address);
  if (!server->address) {
    free(server);
    return -3;
  }
  server->socket_fd = -1;
  server->is_listening = false;
  pthread_mutex_init(&server->mutex, NULL);

  pthread_mutex_lock(&runtime_mutex);
  servers[id] = server;
  pthread_mutex_unlock(&runtime_mutex);

  return id;
}

// Start HTTP server listening - returns 0 on success
int64_t http_listen(int64_t server_id, HttpRequestHandler handler) {
  pthread_mutex_lock(&runtime_mutex);
  HttpServer *server = servers[server_id];
  pthread_mutex_unlock(&runtime_mutex);

  if (!server) {
    return -1;
  }

  // Store the handler function pointer
  server->handler = handler;

  // Create socket
  server->socket_fd = socket(AF_INET, SOCK_STREAM, 0);
  if (server->socket_fd < 0) {
    return -2;
  }

  // Set socket options
  int opt = 1;
  // optval is `const char *` on Winsock, `const void *` on POSIX; the cast is
  // portable to both. [WINDOWS-PORT-PHASE2]
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

  // Spawn a fiber to handle the server loop (non-blocking)
  int64_t fiber_id = fiber_spawn(server_loop_fiber);
  if (fiber_id < 0) {
    server->is_listening = false;
    close(server->socket_fd);
    return -6;
  }

  fprintf(stderr, "HTTP server listening on %s:%d\n", server->address, server->port);

  return 0;
}

// Stop HTTP server - returns 0 on success
int64_t http_stop_server(int64_t server_id) {
  pthread_mutex_lock(&runtime_mutex);
  HttpServer *server = servers[server_id];
  if (server) {
    servers[server_id] = NULL;
    server->is_listening = false;
    if (server->socket_fd >= 0) {
      close(server->socket_fd);
    }
    free(server->address);
    pthread_mutex_destroy(&server->mutex);
    free(server);
  }
  pthread_mutex_unlock(&runtime_mutex);

  return 0;
}
