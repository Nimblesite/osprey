#ifndef HTTP_SHARED_H
#define HTTP_SHARED_H

#include "fiber_api.h"
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

struct HttpPool;

// HTTP Server structure
typedef struct {
  int64_t id;
  int port;
  char *address;
  int socket_fd;
  struct HttpPool *pool; // [HTTP-SERVER-CONNECTIONS] the poller's workers, under mutex
  bool is_listening;
  bool loop_scheduled;
  bool thread_known;
  bool destroy_on_exit;
  bool keeps_process_alive; // [HTTP-SERVER-KEEPALIVE]
  Fiber *server_fiber; // the accept loop, held until the server is destroyed
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

// One field line of an HTTP head: its name before the colon, and its value after
// it with the optional whitespace trimmed (RFC 9110 5.5). Both point into the
// caller's head and are not terminated [HTTP-FIELDS].
typedef struct {
  const char *name;
  size_t name_len;
  const char *value;
  size_t value_len;
} HttpField;

// The first optional whitespace at `start` left off, and the last before `end`.
const char *http_skip_ows(const char *start, const char *end);
const char *http_trim_ows(const char *start, const char *end);
// Whether `length` bytes at `text` spell `expected`, the same ASCII letters in any case.
bool http_text_is(const char *text, size_t length, const char *expected);
// The next field line at or after *cursor in a head of `length` bytes. A line
// ends at CRLF, or where the head ends for its last line; the blank line ends
// the field section. A line without a colon, the request or status line, is
// passed over. Nothing past `length` is read.
bool http_next_field(const char *head, size_t length, size_t *cursor, HttpField *field);
// Whether a comma-separated list (RFC 9110 5.6.1) lists `token` as a whole
// item, in any case.
bool http_value_lists(const char *value, size_t length, const char *token);

// A handle an HTTP server could be registered under.
static inline bool http_valid_server_id(int64_t server_id) {
  return server_id > 0 && server_id < MAX_SERVERS;
}

// [HTTP-SERVER-KEEPALIVE] `http_keep_alive` makes a server hold the process
// open; `http_await_servers` is the wait that hold runs at exit.
int64_t http_keep_alive(int64_t server_id);
void http_await_servers(void);

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
// The largest message a server takes, the longest header, and the largest
// payload sent: room for the echo of the largest message when every byte takes
// the widest JSON escape (\u00XX, twice over: original and echo) (#414).
enum { WS_MESSAGE_LIMIT = 16384, WS_HEADER_MAX = 14, WS_SEND_LIMIT = 2 * 6 * (WS_MESSAGE_LIMIT + 32) + 256 };

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

// What an upgrade request asks for, read as fields (http_next_field) rather
// than found as substrings (#434) [BUILTIN-WEBSOCKET-HANDSHAKE]: RFC 9110 5.1
// makes every field name case-insensitive; RFC 6455 4.2.1 wants `Upgrade` to
// list `websocket` and `Connection` to list the token `Upgrade`, in any case
// and among any others. Repeated lines all count. `key` points into the head,
// key_len bytes, and is "" when the request has none.
typedef struct {
  bool upgrade;    // Upgrade lists websocket
  bool connection; // Connection lists upgrade
  const char *key; // the first Sec-WebSocket-Key
  size_t key_len;
} HttpUpgrade;

HttpUpgrade http_read_upgrade(const char *head, size_t length);
// The 101 of a complete upgrade, or NULL when it is not a handshake: no
// `Upgrade` connection option, or a key that is not a key.
char *create_websocket_accept_response(const HttpUpgrade *upgrade);

// The value of one hexadecimal digit, either case, or -1 for any other byte.
static inline int hex_digit_value(char c) {
  if (c >= '0' && c <= '9') {
    return c - '0';
  }
  if (c >= 'a' && c <= 'f') {
    return c - 'a' + 10;
  }
  return c >= 'A' && c <= 'F' ? c - 'A' + 10 : -1;
}

#endif // HTTP_SHARED_H
