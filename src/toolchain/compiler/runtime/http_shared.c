#include "http_shared.h"
#include <ctype.h>
#include <time.h>

// Standard OpenSSL includes - works on all platforms
#include <openssl/evp.h>
#include <openssl/sha.h>

// OpenSSL 3.5.0+ modern API includes
#include <openssl/buffer.h>

// Winsock initialisation runs once before main() via a
// constructor so every socket call is preceded by WSAStartup. No-op elsewhere.
#ifdef _WIN32
static void osprey_wsa_init_impl(void) {
  WSADATA wsa;
  static bool started = false;
  if (!started && WSAStartup(MAKEWORD(2, 2), &wsa) == 0) {
    started = true;
  }
}

void osprey_wsa_init(void) { osprey_wsa_init_impl(); }

__attribute__((constructor)) static void osprey_wsa_ctor(void) {
  osprey_wsa_init_impl();
}
#endif

// Global runtime state definitions
HttpServer *servers[MAX_SERVERS] = {NULL};
HttpClient *clients[MAX_CLIENTS] = {NULL};
WebSocket *websockets[MAX_WEBSOCKETS] = {NULL};
WebSocketServer *websocket_servers[MAX_WEBSOCKET_SERVERS] = {NULL};
int64_t next_id = 1;
pthread_mutex_t runtime_mutex = PTHREAD_MUTEX_INITIALIZER;

// Thread-safe ID generation
int64_t get_next_id(void) {
  pthread_mutex_lock(&runtime_mutex);
  int64_t id = next_id++;
  pthread_mutex_unlock(&runtime_mutex);
  return id;
}

// [HTTP-SERVER-KEEPALIVE] How often the exit wait looks at the listeners.
#define KEEP_ALIVE_POLL_US 20000

// Whether a server that holds the process open is still listening.
static bool keep_alive_pending(void) {
  bool pending = false;
  pthread_mutex_lock(&runtime_mutex);
  for (int64_t id = 1; id < MAX_SERVERS && !pending; id++) {
    HttpServer *server = servers[id];
    if (server) {
      pthread_mutex_lock(&server->mutex);
      pending = server->keeps_process_alive && server->is_listening;
      pthread_mutex_unlock(&server->mutex);
    }
  }
  pthread_mutex_unlock(&runtime_mutex);
  return pending;
}

// Runs at exit: returns once no holding server is left listening.
void http_await_servers(void) {
  while (keep_alive_pending()) {
    usleep(KEEP_ALIVE_POLL_US);
  }
}

// Marks `server_id` as holding the process open after the program's entry
// returns, as a Node.js process outlives its top-level code while a server is
// open. Stopping the server releases the hold. -1 for an unknown server.
int64_t http_keep_alive(int64_t server_id) {
  static bool armed = false;
  if (!http_valid_server_id(server_id)) {
    return -1;
  }
  pthread_mutex_lock(&runtime_mutex);
  HttpServer *server = servers[server_id];
  if (server) {
    pthread_mutex_lock(&server->mutex);
    server->keeps_process_alive = true;
    pthread_mutex_unlock(&server->mutex);
    armed = armed || atexit(http_await_servers) == 0;
  }
  pthread_mutex_unlock(&runtime_mutex);
  return server && armed ? 0 : -1;
}

// HTTP method to string conversion
const char *http_method_to_string(HttpMethod method) {
  switch (method) {
  case HTTP_GET:
    return "GET";
  case HTTP_POST:
    return "POST";
  case HTTP_PUT:
    return "PUT";
  case HTTP_DELETE:
    return "DELETE";
  case HTTP_PATCH:
    return "PATCH";
  case HTTP_HEAD:
    return "HEAD";
  case HTTP_OPTIONS:
    return "OPTIONS";
  default:
    return "GET";
  }
}

// URL parsing utility
int parse_url(const char *url, char **host, int *port, char **path) {
  if (!url) {
    return -1;
  }

  // Parse URL: scheme://host:port/path. Track whether the scheme is a TLS one
  // so an unspecified port defaults to 443 instead of 80.
  const char *start = url;
  bool secure = false;
  if (strncmp(url, "http://", 7) == 0) {
    start += 7;
  } else if (strncmp(url, "https://", 8) == 0) {
    start += 8;
    secure = true;
  } else if (strncmp(url, "ws://", 5) == 0) {
    start += 5;
  } else if (strncmp(url, "wss://", 6) == 0) {
    start += 6;
    secure = true;
  }

  // Find host end
  const char *slash = strchr(start, '/');
  const char *colon = strchr(start, ':');

  int host_len;
  if (colon && (!slash || colon < slash)) {
    host_len = colon - start;
    *port = atoi(colon + 1);
    if (*port <= 0 || *port > 65535) {
      return -1;
    }
  } else {
    host_len = slash ? slash - start : (int)strlen(start);
    *port = secure ? 443 : 80; // Default port by scheme
  }

  if (host_len <= 0) {
    return -1;
  }

  *host = malloc(host_len + 1);
  strncpy(*host, start, host_len);
  (*host)[host_len] = '\0';

  if (slash) {
    *path = strdup(slash);
  } else {
    *path = strdup("/");
  }

  return 0;
}

// HTTP field lines, read as the RFCs define them, for every head the runtime
// reads [HTTP-FIELDS]. The head is the caller's, `length` bytes: no read goes past it.

const char *http_skip_ows(const char *start, const char *end) {
  while (start < end && (*start == ' ' || *start == '\t')) {
    start++;
  }
  return start;
}

const char *http_trim_ows(const char *start, const char *end) {
  while (end > start && (end[-1] == ' ' || end[-1] == '\t')) {
    end--;
  }
  return end;
}

bool http_text_is(const char *text, size_t length, const char *expected) {
  if (length != strlen(expected)) {
    return false;
  }
  for (size_t index = 0; index < length; index++) {
    if (tolower((unsigned char)text[index]) !=
        tolower((unsigned char)expected[index])) {
      return false;
    }
  }
  return true;
}

// The line from `at` to `stop`, its '\n' or the end of the head, as a field when
// it has a colon: the name before it, the value after it with its blanks trimmed.
static bool http_line_field(const char *head, size_t at, size_t stop, HttpField *field) {
  stop = stop > at && head[stop - 1] == '\r' ? stop - 1 : stop;
  const char *colon = memchr(head + at, ':', stop - at);
  if (!colon) {
    return false;
  }
  const char *value = http_skip_ows(colon + 1, head + stop);
  *field = (HttpField){head + at, (size_t)(colon - head - at), value,
                       (size_t)(http_trim_ows(value, head + stop) - value)};
  return true;
}

bool http_next_field(const char *head, size_t length, size_t *cursor, HttpField *field) {
  while (*cursor < length) {
    size_t at = *cursor;
    const char *eol = memchr(head + at, '\n', length - at);
    size_t stop = eol ? (size_t)(eol - head) : length;
    *cursor = eol ? stop + 1 : length;
    if (http_line_field(head, at, stop, field)) {
      return true;
    }
    if (stop == at || (stop == at + 1 && head[at] == '\r')) {
      return false; // the blank line: the field section is over
    }
  }
  return false;
}

bool http_value_lists(const char *value, size_t length, const char *token) {
  const char *end = value + length;
  for (const char *at = value;;) {
    const char *comma = memchr(at, ',', (size_t)(end - at));
    const char *stop = comma ? comma : end;
    const char *item = http_skip_ows(at, stop);
    bool listed = http_text_is(item, (size_t)(http_trim_ows(item, stop) - item), token);
    if (listed || !comma) {
      return listed;
    }
    at = comma + 1;
  }
}

// Base64 encoding for WebSocket handshake
static const char base64_chars[] =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

char *base64_encode(const unsigned char *data, size_t input_length) {
  size_t output_length = 4 * ((input_length + 2) / 3);
  char *encoded_data = malloc(output_length + 1);
  if (!encoded_data)
    return NULL;

  for (size_t i = 0, j = 0; i < input_length;) {
    uint32_t octet_a = i < input_length ? data[i++] : 0;
    uint32_t octet_b = i < input_length ? data[i++] : 0;
    uint32_t octet_c = i < input_length ? data[i++] : 0;
    uint32_t triple = (octet_a << 0x10) + (octet_b << 0x08) + octet_c;

    encoded_data[j++] = base64_chars[(triple >> 3 * 6) & 0x3F];
    encoded_data[j++] = base64_chars[(triple >> 2 * 6) & 0x3F];
    encoded_data[j++] = base64_chars[(triple >> 1 * 6) & 0x3F];
    encoded_data[j++] = base64_chars[(triple >> 0 * 6) & 0x3F];
  }

  // RFC 4648 §4: a final group short of three input bytes is padded with '='
  // — one for two input bytes, two for one. Without this the trailing zero
  // octets encoded as literal 'A's, so every value whose length was not a
  // multiple of three (a 16-byte Sec-WebSocket-Key, a 20-byte SHA-1 accept
  // token) produced base64 no conforming peer could decode.
  static const size_t pad_for_remainder[] = {0, 2, 1};
  for (size_t p = 0; p < pad_for_remainder[input_length % 3]; p++) {
    encoded_data[output_length - 1 - p] = '=';
  }
  encoded_data[output_length] = '\0';
  return encoded_data;
}

// The runtime's OS-CSPRNG entry point (random_runtime.c) — the handshake nonce
// draws from the same source as `random`, rather than keeping a second one.
void osp_random_bytes(void *buf, size_t len);

// Generate the Sec-WebSocket-Key nonce for a handshake (RFC 6455 §4.1: base64
// of 16 FRESH random bytes). This used to `srand(time(NULL))` on every call and
// draw from `rand()`, so two connections opened in the same second sent the
// IDENTICAL key — a predictable, repeated nonce.
char *generate_websocket_key(void) {
  unsigned char key_bytes[16];
  osp_random_bytes(key_bytes, sizeof(key_bytes));
  return base64_encode(key_bytes, sizeof(key_bytes));
}

// SHA-1 over `<key><RFC 6455 GUID>`, the accept-token digest
// [BUILTIN-WEBSOCKET-HANDSHAKE]. Every refusal and every OpenSSL failure
// leaves `output` fully zeroed through ONE cleanup path: a partial digest must
// never reach the wire, and a caller that ignores the zeroing is caught by
// create_websocket_handshake_response's all-zero check.
enum { SHA1_DIGEST_BYTES = 20, WS_KEY_MAX = 4096 };

void sha1_websocket(const char *input, unsigned char output[SHA1_DIGEST_BYTES]) {
  if (!output) {
    return; // nowhere to write: zeroing a null digest would fault
  }
  memset(output, 0, SHA1_DIGEST_BYTES);
  // SECURITY: reject an absent, empty or over-long key before hashing.
  size_t key_len = input ? strnlen(input, WS_KEY_MAX) : 0;
  if (key_len == 0 || key_len >= WS_KEY_MAX) {
    return;
  }

  static const char websocket_guid[] = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
  size_t guid_len = sizeof(websocket_guid) - 1;
  size_t total_len = key_len + guid_len;
  char *combined = malloc(total_len + 1);
  if (!combined) {
    return;
  }
  memcpy(combined, input, key_len);
  memcpy(combined + key_len, websocket_guid, guid_len);
  combined[total_len] = '\0';

  EVP_MD_CTX *ctx = EVP_MD_CTX_new();
  const EVP_MD *md = ctx ? EVP_sha1() : NULL;
  unsigned int hash_len = SHA1_DIGEST_BYTES;
  bool hashed = md != NULL && EVP_DigestInit_ex(ctx, md, NULL) == 1 &&
                EVP_DigestUpdate(ctx, combined, total_len) == 1 &&
                EVP_DigestFinal_ex(ctx, output, &hash_len) == 1 &&
                hash_len == SHA1_DIGEST_BYTES;
  if (!hashed) {
    memset(output, 0, SHA1_DIGEST_BYTES);
  }
  EVP_MD_CTX_free(ctx); // documented no-op on NULL
  free(combined);
}

// A Sec-WebSocket-Key is exactly 24 base64 characters; anything else is
// refused before it can reach the digest [BUILTIN-WEBSOCKET-HANDSHAKE].
enum { WS_KEY_LEN = 24, WS_ACCEPT_LEN = 28, WS_RESPONSE_SIZE = 512 };

static bool ws_key_is_valid(const char *key) {
  if (!key || strnlen(key, WS_KEY_LEN + 1) != WS_KEY_LEN) {
    return false;
  }
  for (size_t i = 0; i < WS_KEY_LEN; i++) {
    char c = key[i];
    if (!((c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') ||
          (c >= '0' && c <= '9') || c == '+' || c == '/' || c == '=')) {
      return false;
    }
  }
  return true;
}

static bool digest_is_zero(const unsigned char *digest) {
  for (int i = 0; i < SHA1_DIGEST_BYTES; i++) {
    if (digest[i] != 0) {
      return false;
    }
  }
  return true; // sha1_websocket zeroes on every refusal and every failure
}

// The 101 upgrade response carrying Sec-WebSocket-Accept
// [BUILTIN-WEBSOCKET-HANDSHAKE]. NULL on any refusal or failure; the encoded
// accept token is wiped before release on every path.
char *create_websocket_handshake_response(const char *key) {
  if (!ws_key_is_valid(key)) {
    return NULL;
  }
  unsigned char hash[SHA1_DIGEST_BYTES];
  sha1_websocket(key, hash);
  char *encoded = digest_is_zero(hash) ? NULL : base64_encode(hash, sizeof(hash));
  char *response = NULL;
  if (encoded && strnlen(encoded, WS_ACCEPT_LEN + 1) == WS_ACCEPT_LEN) {
    response = calloc(WS_RESPONSE_SIZE, 1);
  }
  if (response) {
    int written = snprintf(response, WS_RESPONSE_SIZE,
                           "HTTP/1.1 101 Switching Protocols\r\n"
                           "Upgrade: websocket\r\n"
                           "Connection: Upgrade\r\n"
                           "Sec-WebSocket-Accept: %s\r\n"
                           "\r\n",
                           encoded);
    if (written < 0 || written >= WS_RESPONSE_SIZE) {
      free(response); // truncated: a half-written handshake is not a handshake
      response = NULL;
    }
  }
  if (encoded) {
    memset(encoded, 0, strlen(encoded));
    free(encoded);
  }
  return response;
}

// One field of an upgrade request: what it asks for, in any case [BUILTIN-WEBSOCKET-HANDSHAKE].
static void read_upgrade_field(const HttpField *field, HttpUpgrade *upgrade) {
  if (http_text_is(field->name, field->name_len, "Upgrade")) {
    upgrade->upgrade = upgrade->upgrade || http_value_lists(field->value, field->value_len, "websocket");
  } else if (http_text_is(field->name, field->name_len, "Connection")) {
    upgrade->connection = upgrade->connection || http_value_lists(field->value, field->value_len, "upgrade");
  } else if (upgrade->key_len == 0 && http_text_is(field->name, field->name_len, "Sec-WebSocket-Key")) {
    upgrade->key = field->value;
    upgrade->key_len = field->value_len;
  }
}

HttpUpgrade http_read_upgrade(const char *head, size_t length) {
  HttpUpgrade upgrade = {.key = ""};
  HttpField field;
  for (size_t cursor = 0; http_next_field(head, length, &cursor, &field);) {
    read_upgrade_field(&field, &upgrade);
  }
  return upgrade;
}

// A key is 24 base64 characters (RFC 6455 4.2.1): a longer value is not one.
enum { WS_KEY_INPUT_MAX = 64 };

char *create_websocket_accept_response(const HttpUpgrade *upgrade) {
  char key[WS_KEY_INPUT_MAX + 1] = {0};
  if (!upgrade->connection || upgrade->key_len > WS_KEY_INPUT_MAX) {
    return NULL;
  }
  memcpy(key, upgrade->key, upgrade->key_len);
  return create_websocket_handshake_response(key);
}
