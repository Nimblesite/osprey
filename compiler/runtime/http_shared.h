#ifndef HTTP_SHARED_H
#define HTTP_SHARED_H

#include <errno.h>
#include <inttypes.h>
#include <openssl/evp.h>
#include <openssl/sha.h>
#include <pthread.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Sockets: Winsock2 on Windows (via the compat header), BSD sockets elsewhere.
// [WINDOWS-PORT-PHASE2]
#ifdef _WIN32
#include "osprey_win_compat.h"
#else
#include <arpa/inet.h>
#include <fcntl.h>
#include <netdb.h>
#include <netinet/in.h>
#include <sys/select.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <unistd.h>
#endif

// Constants
#define MAX_HTTP_BUFFER 8192
#define MAX_SERVERS 100
#define MAX_CLIENTS 1000
#define MAX_WEBSOCKETS 1000
#define MAX_WEBSOCKET_SERVERS 100
#define MAX_CONNECTIONS_PER_SERVER 1000
#define CHUNK_SIZE 4096

// HTTP Method enumeration matching Osprey union types
typedef enum {
  HTTP_GET = 0,
  HTTP_POST = 1,
  HTTP_PUT = 2,
  HTTP_DELETE = 3,
  HTTP_PATCH = 4,
  HTTP_HEAD = 5,
  HTTP_OPTIONS = 6
} HttpMethod;

// HTTP Response structure (matches Osprey HttpResponse type - REMOVED REDUNDANT
// LENGTH FIELDS)
typedef struct HttpResponse {
  int64_t status;
  char *headers;
  char *contentType;
  int64_t streamFd;
  bool isComplete;
  char *partialBody; // Runtime automatically calculates length using strlen()
} HttpResponse;

// Function pointer type for HTTP request handlers
typedef HttpResponse *(*HttpRequestHandler)(char *method, char *path,
                                            char *headers, char *body);

// HTTP Server structure
typedef struct {
  int64_t id;
  int port;
  char *address;
  int socket_fd;
  int active_client_fd;
  bool is_listening;
  bool loop_scheduled;
  bool thread_known;
  bool destroy_on_exit;
  int64_t server_fiber_id;
  pthread_t server_thread;
  pthread_mutex_t mutex;
  HttpRequestHandler handler; // Function pointer to Osprey callback
} HttpServer;

// HTTP Client structure
typedef struct {
  int64_t id;
  char *base_url;
  int timeout;
  char *host;
  int port;
  bool is_persistent;
  bool is_https; // true when the base URL scheme is https:// (use TLS)
} HttpClient;

// WebSocket connection structure
typedef struct {
  int64_t id;
  char *url;
  char *message_handler;
  int socket_fd;
  bool is_connected;
  pthread_t receiver_thread;
  pthread_mutex_t mutex;
} WebSocket;

// WebSocket Server structure
typedef struct {
  int64_t id;
  int port;
  char *address;
  char *path; // WebSocket endpoint path (e.g., "/chat")
  int socket_fd;
  bool is_listening;
  pthread_t server_thread;
  pthread_mutex_t mutex;
  WebSocket *connections[MAX_CONNECTIONS_PER_SERVER];
  int connection_count;
} WebSocketServer;

// Global runtime state
extern HttpServer *servers[MAX_SERVERS];
extern HttpClient *clients[MAX_CLIENTS];
extern WebSocket *websockets[MAX_WEBSOCKETS];
extern WebSocketServer *websocket_servers[MAX_WEBSOCKET_SERVERS];
extern int64_t next_id;
extern pthread_mutex_t runtime_mutex;

// Shared utility functions
const char *http_method_to_string(HttpMethod method);
int parse_url(const char *url, char **host, int *port, char **path);
char *base64_encode(const unsigned char *data, size_t input_length);
char *generate_websocket_key(void);
int64_t get_next_id(void);

// WebSocket frame functions (shared between client and server)
int send_websocket_frame(int socket_fd, const char *payload);
int parse_websocket_frame(const char *frame_data, size_t frame_len,
                          char **payload);
char *create_websocket_handshake_response(const char *key);

#endif // HTTP_SHARED_H
