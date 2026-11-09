#ifndef HTTP_SHARED_H
#define HTTP_SHARED_H

#include "socket_platform.h"
#include <openssl/evp.h>
#include <openssl/sha.h>

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
  int socket_fd;
  bool is_connected;
  pthread_mutex_t mutex;
} WebSocket;

// WebSocket Server structure
typedef struct {
  int64_t id;
  int port;
  char *address;
  char *path; // WebSocket endpoint path (e.g., "/chat")
  int socket_fd;
  atomic_bool is_listening;
  bool thread_started;
  pthread_t server_thread;
  pthread_mutex_t mutex; // guards the three below
  pthread_cond_t handlers_gone;
  WebSocket *connections[MAX_CONNECTIONS_PER_SERVER]; // the upgraded, live ones
  int connection_count;
  int handler_count; // threads that may still read this server, upgraded or not
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

// RFC 6455 frames, websocket_frames.c [BUILTIN-WEBSOCKET-FRAMING].
enum {
  WS_OP_CONTINUATION = 0x0,
  WS_OP_TEXT = 0x1,
  WS_OP_BINARY = 0x2,
  WS_OP_CLOSE = 0x8,
  WS_OP_PING = 0x9,
  WS_OP_PONG = 0xA
};
// What parse_websocket_frame answers when it takes no frame.
enum { WS_FRAME_PARTIAL = 0, WS_FRAME_INVALID = -1, WS_FRAME_TOO_BIG = -2, WS_FRAME_NO_MEMORY = -3 };
// Status codes the server closes with (RFC 6455 §7.4.1).
enum { WS_CLOSE_PROTOCOL = 1002, WS_CLOSE_UNSUPPORTED = 1003, WS_CLOSE_TOO_BIG = 1009, WS_CLOSE_INTERNAL = 1011 };
// The largest payload sent, the largest message a server takes, the longest header.
enum { WS_SEND_LIMIT = 4096, WS_MESSAGE_LIMIT = 16384, WS_HEADER_MAX = 14 };

typedef struct {
  bool fin;
  bool masked;
  unsigned char opcode;
  char *payload; // NUL-terminated copy the caller frees
  size_t payload_len;
} WsFrame;

int64_t parse_websocket_frame(const char *data, size_t len, size_t limit, WsFrame *frame);
size_t ws_encode_frame(unsigned char *out, unsigned char opcode, const char *payload, size_t len,
                       const unsigned char *key);
int send_websocket_message(int socket_fd, unsigned char opcode, const char *payload, size_t len, bool masked);
int send_websocket_frame(int socket_fd, const char *payload); // server text frame
int send_websocket_close(int socket_fd, int code);            // 0: no status

// One client's frames as the server reads them: bytes in, whole messages out.
typedef struct {
  char data[WS_MESSAGE_LIMIT + WS_HEADER_MAX];
  size_t held;
  char *message; // the fragments received so far
  size_t message_len;
  bool in_message;
} WsStream;

typedef enum { WS_EVENT_NONE, WS_EVENT_MESSAGE, WS_EVENT_PING, WS_EVENT_CLOSE } WsEventKind;

typedef struct {
  WsEventKind kind;
  char *payload; // a message or a ping's data, the caller frees
  size_t payload_len;
  int close_code; // WS_EVENT_CLOSE: the status to answer with, 0 for none
} WsEvent;

WsEvent ws_stream_next(WsStream *stream);
void ws_stream_release(WsStream *stream);
char *create_websocket_handshake_response(const char *key);

#endif // HTTP_SHARED_H
