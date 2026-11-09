// RFC 6455 frames [BUILTIN-WEBSOCKET-FRAMING]: the codec both transports share,
// and the stream a server reads one client's frames from, which reassembles
// fragmented messages and surfaces pings and the closing handshake.
#include "http_shared.h"
#include "socket_safety.h"

void osp_random_bytes(void *buf, size_t len);

enum { WS_FIN = 0x80, WS_RESERVED = 0x70, WS_CONTROL = 0x08, WS_MASK_BIT = 0x80, WS_KEY_LEN = 4 };

// ----------------------------------------------------------------- decoding

static bool ws_known_opcode(unsigned char opcode) {
  return opcode <= WS_OP_BINARY || (opcode >= WS_OP_CLOSE && opcode <= WS_OP_PONG);
}

// FIN, opcode and mask from the first two bytes; INVALID for reserved bits, an
// unknown opcode or a fragmented control frame.
static int64_t ws_read_header(const unsigned char *data, WsFrame *frame) {
  frame->fin = (data[0] & WS_FIN) != 0;
  frame->opcode = data[0] & 0x0F;
  frame->masked = (data[1] & WS_MASK_BIT) != 0;
  bool reserved = (data[0] & WS_RESERVED) != 0;
  bool fragmented_control = (frame->opcode & WS_CONTROL) && !frame->fin;
  return reserved || !ws_known_opcode(frame->opcode) || fragmented_control ? WS_FRAME_INVALID : 1;
}

// The payload length in whichever of the three forms it was sent, and where the
// masking key starts. A longer form than the length needs, or a 64-bit length
// with its top bit set, is INVALID.
static int64_t ws_read_length(const unsigned char *data, size_t len, uint64_t *length, size_t *offset) {
  uint64_t base = data[1] & 0x7F;
  size_t width = base == 126 ? 2 : base == 127 ? 8 : 0;
  if (len < 2 + width) {
    return WS_FRAME_PARTIAL;
  }
  *length = width ? 0 : base;
  for (size_t i = 0; i < width; i++) {
    *length = (*length << 8) | data[2 + i];
  }
  *offset = 2 + width;
  bool minimal = width == 0 || (width == 2 ? *length >= 126 : *length > 0xFFFF);
  return minimal && (*length >> 63) == 0 ? 1 : WS_FRAME_INVALID;
}

// The unmasked payload, NUL-terminated, into `frame`.
static bool ws_copy_payload(const unsigned char *at, bool masked, size_t length, WsFrame *frame) {
  const unsigned char *body = at + (masked ? WS_KEY_LEN : 0);
  frame->payload = malloc(length + 1);
  if (!frame->payload) {
    return false;
  }
  for (size_t i = 0; i < length; i++) {
    frame->payload[i] = (char)(body[i] ^ (masked ? at[i % WS_KEY_LEN] : 0));
  }
  frame->payload[length] = '\0';
  frame->payload_len = length;
  return true;
}

// The header's facts, checked: 1 when they allow a payload of `*length` bytes
// at `*offset`, else the refusal or PARTIAL.
static int64_t ws_frame_bounds(const unsigned char *bytes, size_t len, size_t limit, WsFrame *frame,
                               uint64_t *length, size_t *offset) {
  int64_t status = len < 2 ? WS_FRAME_PARTIAL : ws_read_header(bytes, frame);
  status = status == 1 ? ws_read_length(bytes, len, length, offset) : status;
  if (status == 1 && (frame->opcode & WS_CONTROL) && *length > 125) {
    return WS_FRAME_INVALID; // control frames carry at most 125 bytes
  }
  return status == 1 && *length > limit ? WS_FRAME_TOO_BIG : status;
}

// One frame from the front of `data`: the bytes it took, or PARTIAL while only
// part of one is there, INVALID for a frame RFC 6455 forbids, TOO_BIG for a
// payload over `limit`. A refused or partial frame allocates nothing.
int64_t parse_websocket_frame(const char *data, size_t len, size_t limit, WsFrame *frame) {
  const unsigned char *bytes = (const unsigned char *)data;
  uint64_t length = 0;
  size_t offset = 0;
  int64_t status = ws_frame_bounds(bytes, len, limit, frame, &length, &offset);
  if (status != 1) {
    return status;
  }
  size_t key = frame->masked ? WS_KEY_LEN : 0;
  if (len - offset < key || len - offset - key < length) {
    return WS_FRAME_PARTIAL;
  }
  bool copied = ws_copy_payload(bytes + offset, frame->masked, (size_t)length, frame);
  return copied ? (int64_t)(offset + key + length) : WS_FRAME_NO_MEMORY;
}

// ----------------------------------------------------------------- encoding

// The length field in the shortest form that holds `len`, after the mask bit.
static size_t ws_put_length(unsigned char *out, size_t len, bool masked) {
  size_t width = len < 126 ? 0 : len <= 0xFFFF ? 2 : 8;
  unsigned char form = (unsigned char)(width == 0 ? len : width == 2 ? 126 : 127);
  out[0] = (unsigned char)((masked ? WS_MASK_BIT : 0) | form);
  for (size_t i = 0; i < width; i++) {
    out[1 + i] = (unsigned char)((uint64_t)len >> (8 * (width - 1 - i)));
  }
  return 1 + width;
}

// One whole frame into `out` (room for WS_HEADER_MAX + len), masked with `key`
// when there is one; the bytes written.
size_t ws_encode_frame(unsigned char *out, unsigned char opcode, const char *payload, size_t len,
                       const unsigned char *key) {
  out[0] = (unsigned char)(WS_FIN | opcode);
  size_t at = 1 + ws_put_length(out + 1, len, key != NULL);
  if (key) {
    memcpy(out + at, key, WS_KEY_LEN);
    at += WS_KEY_LEN;
  }
  for (size_t i = 0; i < len; i++) {
    out[at + i] = (unsigned char)((unsigned char)payload[i] ^ (key ? key[i % WS_KEY_LEN] : 0));
  }
  return at + len;
}

// A frame goes whole or not at all: part of one corrupts the stream for good.
// Clients mask every frame with a fresh key (RFC 6455 §5.3); servers never do.
int send_websocket_message(int socket_fd, unsigned char opcode, const char *payload, size_t len, bool masked) {
  if (!payload || len > WS_SEND_LIMIT) {
    return -1;
  }
  unsigned char frame[WS_SEND_LIMIT + WS_HEADER_MAX];
  unsigned char key[WS_KEY_LEN];
  if (masked) {
    osp_random_bytes(key, sizeof key);
  }
  size_t frame_len = ws_encode_frame(frame, opcode, payload, len, masked ? key : NULL);
  for (size_t sent = 0; sent < frame_len;) {
    int n = osp_socket_send(socket_fd, (const char *)frame + sent, frame_len - sent);
    if (n <= 0) {
      return -1;
    }
    sent += (size_t)n;
  }
  return (int)frame_len;
}

int send_websocket_frame(int socket_fd, const char *payload) {
  return payload ? send_websocket_message(socket_fd, WS_OP_TEXT, payload, strlen(payload), false) : -1;
}

// A close frame carrying `code`, or no status at all for 0.
int send_websocket_close(int socket_fd, int code) {
  const char body[2] = {(char)(code >> 8), (char)(code & 0xFF)};
  return send_websocket_message(socket_fd, WS_OP_CLOSE, body, code ? 2 : 0, false);
}

// ------------------------------------------------------------------ stream

static WsEvent ws_event(WsEventKind kind, char *payload, size_t len, int code) {
  return (WsEvent){.kind = kind, .payload = payload, .payload_len = len, .close_code = code};
}

// The connection ends: the frame goes, and the server answers with `code`.
static WsEvent ws_refuse(WsFrame *frame, int code) {
  free(frame->payload);
  return ws_event(WS_EVENT_CLOSE, NULL, 0, code);
}

// Status codes a peer may send (RFC 6455 §7.4): the defined ones that are not
// reserved for local use, and the 3000-4999 range for libraries and applications.
static bool ws_sendable_code(int code) {
  return (code >= 1000 && code <= 1003) || (code >= 1007 && code <= 1014) || (code >= 3000 && code <= 4999);
}

// The peer's close: answered with its own status, a malformed one with 1002.
static WsEvent ws_peer_close(WsFrame *frame) {
  const unsigned char *body = (const unsigned char *)frame->payload;
  int code = frame->payload_len >= 2 ? (body[0] << 8) | body[1] : 0;
  bool malformed = frame->payload_len == 1 || (frame->payload_len >= 2 && !ws_sendable_code(code));
  return ws_refuse(frame, malformed ? WS_CLOSE_PROTOCOL : code);
}

// A fragment joins the message in progress; the last one delivers it whole.
static WsEvent ws_append(WsStream *stream, WsFrame *frame) {
  size_t total = stream->message_len + frame->payload_len;
  char *grown = total > WS_MESSAGE_LIMIT ? NULL : realloc(stream->message, total + 1);
  if (!grown) {
    return ws_refuse(frame, total > WS_MESSAGE_LIMIT ? WS_CLOSE_TOO_BIG : WS_CLOSE_INTERNAL);
  }
  memcpy(grown + stream->message_len, frame->payload, frame->payload_len + 1);
  free(frame->payload);
  stream->message = grown;
  stream->message_len = total;
  stream->in_message = !frame->fin;
  if (stream->in_message) {
    return ws_event(WS_EVENT_NONE, NULL, 0, 0);
  }
  WsEvent whole = ws_event(WS_EVENT_MESSAGE, stream->message, total, 0);
  stream->message = NULL;
  stream->message_len = 0;
  return whole;
}

// Text, whole or in fragments. Continuing nothing, or starting a message while
// another is in fragments, is a protocol error; binary is not taken (1003).
static WsEvent ws_data(WsStream *stream, WsFrame *frame) {
  bool continuation = frame->opcode == WS_OP_CONTINUATION;
  if (continuation != stream->in_message) {
    return ws_refuse(frame, WS_CLOSE_PROTOCOL);
  }
  if (frame->opcode == WS_OP_BINARY) {
    return ws_refuse(frame, WS_CLOSE_UNSUPPORTED);
  }
  if (frame->fin && !continuation) {
    return ws_event(WS_EVENT_MESSAGE, frame->payload, frame->payload_len, 0);
  }
  return ws_append(stream, frame);
}

// What one client frame means to the server. Clients MUST mask (RFC 6455 §5.1).
static WsEvent ws_take(WsStream *stream, WsFrame *frame) {
  if (!frame->masked) {
    return ws_refuse(frame, WS_CLOSE_PROTOCOL);
  }
  switch (frame->opcode) {
  case WS_OP_PING:
    return ws_event(WS_EVENT_PING, frame->payload, frame->payload_len, 0);
  case WS_OP_PONG:
    free(frame->payload); // an unsolicited pong needs no answer
    return ws_event(WS_EVENT_NONE, NULL, 0, 0);
  case WS_OP_CLOSE:
    return ws_peer_close(frame);
  default:
    return ws_data(stream, frame);
  }
}

// Why a stream cannot go on: PARTIAL waits for bytes, the rest end the connection.
static WsEvent ws_stalled(int64_t status) {
  switch (status) {
  case WS_FRAME_PARTIAL:
    return ws_event(WS_EVENT_NONE, NULL, 0, 0);
  case WS_FRAME_TOO_BIG:
    return ws_event(WS_EVENT_CLOSE, NULL, 0, WS_CLOSE_TOO_BIG);
  case WS_FRAME_NO_MEMORY:
    return ws_event(WS_EVENT_CLOSE, NULL, 0, WS_CLOSE_INTERNAL);
  default:
    return ws_event(WS_EVENT_CLOSE, NULL, 0, WS_CLOSE_PROTOCOL);
  }
}

// The next thing the server must act on from the bytes held: a whole message,
// a ping, the end of the connection, or NONE until more bytes arrive.
WsEvent ws_stream_next(WsStream *stream) {
  for (;;) {
    WsFrame frame = {0};
    int64_t used = parse_websocket_frame(stream->data, stream->held, WS_MESSAGE_LIMIT, &frame);
    if (used <= 0) {
      return ws_stalled(used);
    }
    stream->held -= (size_t)used;
    memmove(stream->data, stream->data + used, stream->held);
    WsEvent event = ws_take(stream, &frame);
    if (event.kind != WS_EVENT_NONE) {
      return event;
    }
  }
}

void ws_stream_release(WsStream *stream) {
  if (stream) {
    free(stream->message);
    free(stream);
  }
}
