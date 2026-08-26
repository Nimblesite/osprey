#include "http_shared.h"
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

// WebSocket frame encoding (shared)
int send_websocket_frame(int socket_fd, const char *payload) {
  if (!payload)
    return -1;

  size_t payload_len = strlen(payload);
  if (payload_len > 4096)
    return -1; // Prevent DoS attacks

  unsigned char frame[4106]; // Fixed size: 4096 + 10 for header
  int frame_len = 0;

  // Opcode: 0x1 for text frame
  frame[frame_len++] = 0x81;

  // Payload length. The 4096-byte guard above already bounds payload_len well
  // under the 16-bit extended form's 65535 limit, so the 64-bit form is
  // unreachable here and is not emitted [BUILTIN-WEBSOCKET-FRAMING].
  if (payload_len < 126) {
    frame[frame_len++] = payload_len;
  } else {
    frame[frame_len++] = 126;
    frame[frame_len++] = (payload_len >> 8) & 0xFF;
    frame[frame_len++] = payload_len & 0xFF;
  }

  // Copy payload
  memcpy(frame + frame_len, payload, payload_len);
  frame_len += payload_len;

  // `frame` is unsigned char; Winsock's send() takes `const char *` (POSIX
  // takes `const void *`). Cast at the call site — portable both ways, mirrors
  // the setsockopt/accept/bind casts.
  return send(socket_fd, (const char *)frame, frame_len, 0);
}

// WebSocket frame parsing (shared)
int parse_websocket_frame(const char *frame_data, size_t frame_len,
                          char **payload) {
  if (frame_len < 2)
    return -1;

  // Extract only the values we actually use in this simplified implementation
  unsigned char second_byte = frame_data[1];
  bool mask = (second_byte & 0x80) != 0;
  size_t payload_len = second_byte & 0x7F;

  // NOTE: This is a simplified WebSocket frame parser that ignores:
  // - FIN bit (frame_data[0] & 0x80): fragmentation handling
  // - Opcode (frame_data[0] & 0x0F): frame type (text, binary, close, ping,
  // pong)

  size_t offset = 2;

  // Extended payload length. The bytes must be read UNSIGNED: `char` is
  // signed here, and a length byte >= 0x80 would sign-extend into a huge
  // size_t, rejecting every extended frame whose low length byte has the top
  // bit set (e.g. any 128..255-byte payload).
  if (payload_len == 126) {
    if (frame_len < offset + 2)
      return -1;
    payload_len = ((size_t)(unsigned char)frame_data[offset] << 8) |
                  (size_t)(unsigned char)frame_data[offset + 1];
    offset += 2;
  } else if (payload_len == 127) {
    // Not implemented for this simple version
    return -1;
  }

  // Masking key
  unsigned char masking_key[4] = {0};
  if (mask) {
    if (frame_len < offset + 4)
      return -1;
    memcpy(masking_key, frame_data + offset, 4);
    offset += 4;
  }

  // Payload
  if (frame_len < offset + payload_len)
    return -1;

  *payload = malloc(payload_len + 1);
  if (!*payload)
    return -1;

  for (size_t i = 0; i < payload_len; i++) {
    (*payload)[i] = frame_data[offset + i];
    if (mask) {
      (*payload)[i] ^= masking_key[i % 4];
    }
  }
  (*payload)[payload_len] = '\0';

  return payload_len;
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
