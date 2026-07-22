#include "http_shared.h"
#include <time.h>

// Standard OpenSSL includes - works on all platforms
#include <openssl/evp.h>
#include <openssl/sha.h>

// OpenSSL 3.5.0+ modern API includes
#include <openssl/buffer.h>

// Winsock initialisation — [WINDOWS-PORT-PHASE2]. Runs once before main() via a
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

  encoded_data[output_length] = '\0';
  return encoded_data;
}

// Generate WebSocket key for handshake
char *generate_websocket_key(void) {
  unsigned char key_bytes[16];
  srand(time(NULL));
  for (int i = 0; i < 16; i++) {
    key_bytes[i] = rand() % 256;
  }
  return base64_encode(key_bytes, 16);
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

  // Payload length
  if (payload_len < 126) {
    frame[frame_len++] = payload_len;
  } else if (payload_len < 65536) {
    frame[frame_len++] = 126;
    frame[frame_len++] = (payload_len >> 8) & 0xFF;
    frame[frame_len++] = payload_len & 0xFF;
  } else {
    return -1; // Payload too large for this implementation
  }

  // Copy payload
  memcpy(frame + frame_len, payload, payload_len);
  frame_len += payload_len;

  // `frame` is unsigned char; Winsock's send() takes `const char *` (POSIX
  // takes `const void *`). Cast at the call site — portable both ways, mirrors
  // the setsockopt/accept/bind casts. [WINDOWS-PORT-PHASE2]
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

  // Extended payload length
  if (payload_len == 126) {
    if (frame_len < offset + 2)
      return -1;
    payload_len = (frame_data[offset] << 8) | frame_data[offset + 1];
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

// Modern OpenSSL 3.5.0+ SHA-1 implementation using EVP API 🛡️💀
void sha1_websocket(const char *input, unsigned char output[20]) {
  // SECURITY: Validate input parameters with strict validation
  if (!input || !output) {
    memset(output, 0, 20);
    return;
  }

  // SECURITY: Enforce maximum key length to prevent attacks
  size_t key_len = strnlen(input, 4096);
  if (key_len >= 4096 || key_len == 0) {
    memset(output, 0, 20);
    return;
  }

  const char *websocket_guid = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
  size_t guid_len = strlen(websocket_guid);

  // Create concatenated string for hashing
  size_t total_len = key_len + guid_len;
  char *combined = malloc(total_len + 1);
  if (!combined) {
    memset(output, 0, 20);
    return;
  }

  memcpy(combined, input, key_len);
  memcpy(combined + key_len, websocket_guid, guid_len);
  combined[total_len] = '\0';

  // OpenSSL 3.5 EVP API implementation
  EVP_MD_CTX *ctx = EVP_MD_CTX_new();
  if (!ctx) {
    free(combined);
    memset(output, 0, 20);
    return;
  }

  const EVP_MD *md = EVP_sha1();
  if (!md) {
    EVP_MD_CTX_free(ctx);
    free(combined);
    memset(output, 0, 20);
    return;
  }

  // Initialize digest context
  if (EVP_DigestInit_ex(ctx, md, NULL) != 1) {
    EVP_MD_CTX_free(ctx);
    free(combined);
    memset(output, 0, 20);
    return;
  }

  // Update digest with data
  if (EVP_DigestUpdate(ctx, combined, total_len) != 1) {
    EVP_MD_CTX_free(ctx);
    free(combined);
    memset(output, 0, 20);
    return;
  }

  // Finalize digest
  unsigned int hash_len = 20;
  if (EVP_DigestFinal_ex(ctx, output, &hash_len) != 1 || hash_len != 20) {
    EVP_MD_CTX_free(ctx);
    free(combined);
    memset(output, 0, 20);
    return;
  }

  // Clean up
  EVP_MD_CTX_free(ctx);
  free(combined);
}

// TITANIUM-ARMORED WebSocket handshake response with GUN TURRET VALIDATION
// 🛡️💀
char *create_websocket_handshake_response(const char *key) {
  // SECURITY: Bulletproof input validation
  if (!key)
    return NULL;

  // SECURITY: Validate WebSocket key format (must be 24 chars base64)
  size_t key_len = strnlen(key, 256);
  if (key_len != 24)
    return NULL;

  // SECURITY: Validate base64 characters only
  for (size_t i = 0; i < key_len; i++) {
    char c = key[i];
    if (!((c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') ||
          (c >= '0' && c <= '9') || c == '+' || c == '/' || c == '=')) {
      return NULL;
    }
  }

  // SECURITY: Generate SHA-1 hash with military-grade protection
  unsigned char hash[20];
  memset(hash, 0, sizeof(hash));
  sha1_websocket(key, hash);

  // SECURITY: Verify hash was generated successfully
  bool hash_valid = false;
  for (int i = 0; i < 20; i++) {
    if (hash[i] != 0) {
      hash_valid = true;
      break;
    }
  }
  if (!hash_valid)
    return NULL;

  // SECURITY: Base64 encode with error checking
  char *encoded_hash = base64_encode(hash, 20);
  if (!encoded_hash)
    return NULL;

  // SECURITY: Validate encoded hash length (SHA-1 base64 = 28 chars)
  if (strnlen(encoded_hash, 64) != 28) {
    free(encoded_hash);
    return NULL;
  }

  // SECURITY: Use secure buffer allocation with bounds checking
  const size_t response_size = 512;
  char *response = calloc(response_size, 1);
  if (!response) {
    memset(encoded_hash, 0, strlen(encoded_hash));
    free(encoded_hash);
    return NULL;
  }

  // SECURITY: Use safe formatted output with bounds checking
  int written = snprintf(response, response_size,
                         "HTTP/1.1 101 Switching Protocols\r\n"
                         "Upgrade: websocket\r\n"
                         "Connection: Upgrade\r\n"
                         "Sec-WebSocket-Accept: %s\r\n"
                         "\r\n",
                         encoded_hash);

  // SECURITY: Verify output was not truncated
  if (written < 0 || written >= (int)response_size) {
    memset(response, 0, response_size);
    free(response);
    response = NULL;
  }

  // SECURITY: Zero out sensitive memory
  memset(encoded_hash, 0, strlen(encoded_hash));
  free(encoded_hash);

  return response;
}
