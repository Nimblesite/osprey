// The scripted origin the HTTP client suites share, split from the tests themselves
// so each test TU stays under the repo size budget.
#include "test_loopback.h"
#include "http_runtime_tests.h"
#include "http_shared.h"
#include "io_error.h"
#include "socket_safety.h"

#include <assert.h>
#include <stdio.h>

// ---------------------------------------------------------------------------
// Scripted origin: a raw TCP server that answers with EXACT bytes. It lets the
// client be asserted against wire responses the Osprey server would never
// produce -- chunked framing, a bogus status line, a silent close -- and
// records the request the client actually sent [HTTP-STATUS-CLIENT].
// ---------------------------------------------------------------------------

// Reads one whole HTTP request through `receive`: headers, then
// Content-Length bytes of body. Plain and TLS origins share it.
void origin_read_request_via(void *conn, origin_receive receive, char *out, size_t capacity) {
  size_t len = 0;
  char *blank = NULL;
  while (len + 1 < capacity && blank == NULL) {
    long n = receive(conn, out + len, capacity - 1 - len);
    if (n <= 0) {
      break;
    }
    len += (size_t)n;
    out[len] = '\0';
    blank = strstr(out, "\r\n\r\n");
  }
  if (blank == NULL) {
    return;
  }
  const char *marker = strstr(out, "Content-Length: ");
  size_t want = marker ? (size_t)atoi(marker + 16) : 0;
  size_t have = len - (size_t)(blank + 4 - out);
  while (have < want && len + 1 < capacity) {
    long n = receive(conn, out + len, capacity - 1 - len);
    if (n <= 0) {
      break;
    }
    len += (size_t)n;
    have += (size_t)n;
    out[len] = '\0';
  }
}

static long socket_receive(void *conn, char *out, size_t capacity) {
  return (long)recv(*(int *)conn, out, capacity, 0);
}

void origin_read_request(int fd, char *out, size_t capacity) {
  origin_read_request_via(&fd, socket_receive, out, capacity);
}

// A reader slow enough to keep a sender's every wait inside its timeout, until
// the sender gives up: trickle_bytes per tick, trickle_us apart. A tick must free
// more than two segments of the receive window, or TCP holds back the window
// update and the sender stalls however fast this reads.
static void origin_trickle(int fd, const ScriptedOrigin *origin) {
  char scratch[MAX_HTTP_BUFFER];
  for (;;) {
    for (size_t taken = 0; taken < origin->trickle_bytes;) {
      size_t left = origin->trickle_bytes - taken;
      ssize_t n = recv(fd, scratch, left < sizeof scratch ? left : sizeof scratch, 0);
      if (n <= 0) {
        return;
      }
      taken += (size_t)n;
    }
    usleep(origin->trickle_us);
  }
}

// What the origin reads before it replies: one HTTP request, one raw segment, or,
// trickling, everything the client sends.
static void origin_take_request(int fd, ScriptedOrigin *origin) {
  if (origin->trickle_bytes) {
    origin_trickle(fd, origin);
    origin->reply = NULL;
  } else if (origin->raw_request) {
    ssize_t n = recv(fd, origin->request, sizeof(origin->request) - 1, 0);
    origin->request[n > 0 ? n : 0] = '\0';
  } else {
    origin_read_request(fd, origin->request, sizeof(origin->request));
  }
}

static void *origin_thread(void *arg) {
  ScriptedOrigin *origin = (ScriptedOrigin *)arg;
  int fd = accept(origin->listen_fd, NULL, NULL);
  if (fd < 0) {
    return NULL;
  }
  origin_take_request(fd, origin);
  usleep(origin->reply_delay_us);
  if (origin->reply) {
    size_t total = origin->reply_len;
    size_t sent = 0;
    while (sent < total) {
      ssize_t n = osp_socket_send(fd, origin->reply + sent,
                                  origin->drip_us ? 1 : total - sent);
      if (n <= 0) {
        break;
      }
      sent += (size_t)n;
      usleep(origin->drip_us);
    }
  }
  char byte;
  while (origin->hold_open && recv(fd, &byte, 1, 0) > 0) {
  }
  close(fd); // Connection: close is what makes the client stop draining
  return NULL;
}

void origin_start(ScriptedOrigin *origin, const char *reply) {
  origin_start_bytes(origin, reply, reply ? strlen(reply) : 0);
}

void origin_start_bytes(ScriptedOrigin *origin, const char *reply, size_t len) {
  memset(origin, 0, sizeof(*origin));
  origin->reply = reply;
  origin->reply_len = len;
  origin->listen_fd = test_loopback_listen(ORIGIN_PORT);
  assert(pthread_create(&origin->thread, NULL, origin_thread, origin) == 0);
}

void origin_stop(ScriptedOrigin *origin) {
  assert(pthread_join(origin->thread, NULL) == 0);
  close(origin->listen_fd);
}

int64_t origin_client(void) {
  int64_t client_id = http_create_client(
      (char *)(uintptr_t) "http://127.0.0.1:18088", 2000);
  assert(client_id > 0);
  return client_id;
}

// Handles are never reused, and servers share their numbering with clients, so
// these hundreds of cases share one client rather than spend hundreds of handles.
int64_t shared_origin_client;
bool origin_holds_connections;
unsigned origin_drip_us;

int64_t origin_exchange(const char *reply, size_t len) {
  ScriptedOrigin origin;
  origin_start_bytes(&origin, reply, len);
  origin.hold_open = origin_holds_connections; // read only once the request arrives
  origin.drip_us = origin_drip_us;
  int64_t handle = http_get_response(shared_origin_client, (char *)(uintptr_t) "/f",
                                     (char *)(uintptr_t) "");
  origin_stop(&origin);
  return handle;
}

void expect_response_bytes_n(const char *reply, size_t len, int64_t status,
                             const char *body, size_t body_len) {
  long live = test_alloc_live();
  osp_io_error_clear();
  int64_t handle = origin_exchange(reply, len);
  if (handle < 1) {
    fprintf(stderr, "got %lld (%s) from: %.*s\n", (long long)handle,
            osp_io_error() ? osp_io_error() : "no message", len > 200 ? 200 : (int)len, reply);
  }
  assert(handle >= 1);
  assert(osp_io_error() == NULL);
  assert(http_response_status(handle) == status);
  long before_copy = test_alloc_live();
  char *got = http_response_body(handle);
  long copy_delta = test_alloc_live() - before_copy;
  assert(http_response_free(handle) == 0);
  assert(got != NULL && osp_strlen(got) == (int64_t)body_len);
  assert(memcmp(got, body, body_len) == 0 && got[body_len] == '\0');
  osp_release(got);
  // Exclude the managed copy and any collection it triggered. The raw
  // transport and handle allocations must still balance exactly.
  assert(test_alloc_live() == live + copy_delta);
}

void expect_response_n(const char *reply, size_t len, int64_t status,
                       const char *body) {
  expect_response_bytes_n(reply, len, status, body, strlen(body));
}

// Answered with this code and message, nothing leaked, the client still usable.
void expect_failure_n(const char *reply, size_t len, int64_t code,
                             const char *message) {
  static const char good[] = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
  long live = test_alloc_live();
  osp_io_error_clear();
  int64_t got = origin_exchange(reply, len);
  if (got != code) {
    fprintf(stderr, "got %lld wanting %lld (%s) from: %.*s\n", (long long)got, (long long)code,
            osp_io_error() ? osp_io_error() : "no message", len > 200 ? 200 : (int)len, reply);
  }
  assert(got == code);
  assert(osp_io_error() != NULL && strcmp(osp_io_error(), message) == 0);
  assert(test_alloc_live() == live);
  osp_io_error_clear();
  int64_t handle = origin_exchange(good, sizeof(good) - 1);
  assert(handle >= 1 && osp_io_error() == NULL);
  assert(http_response_free(handle) == 0);
}

void expect_refused_n(const char *reply, size_t len, const char *message) {
  expect_failure_n(reply, len, -14, message);
}
