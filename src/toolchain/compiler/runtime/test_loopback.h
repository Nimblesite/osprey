// Loopback sockets the HTTP, TLS and WebSocket C suites share: one address
// builder and one single-connection listener, so no suite re-spells them.
#ifndef OSPREY_TEST_LOOPBACK_H
#define OSPREY_TEST_LOOPBACK_H

#include "http_shared.h"

#include <assert.h>
#include <string.h>

static inline struct sockaddr_in test_loopback_addr(int port) {
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof(addr));
  addr.sin_family = AF_INET;
  addr.sin_port = htons((uint16_t)port);
  addr.sin_addr.s_addr = inet_addr("127.0.0.1");
  return addr;
}

// A reusable loopback listener on `port` (0 = OS-assigned [CI-TEST-PORTS])
// with `backlog` pending connections. The port actually bound lands in
// *bound when it is not NULL, so suites running beside each other never
// collide on a hard-coded number.
static inline int test_loopback_bind(int port, int backlog, int *bound) {
  int fd = socket(AF_INET, SOCK_STREAM, 0);
  assert(fd >= 0);
  int opt = 1;
  assert(setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, (const char *)&opt,
                    sizeof(opt)) == 0);
  struct sockaddr_in addr = test_loopback_addr(port);
  assert(bind(fd, (struct sockaddr *)&addr, sizeof(addr)) == 0);
  assert(listen(fd, backlog) == 0);
  if (bound != NULL) {
    socklen_t len = sizeof(addr);
    assert(getsockname(fd, (struct sockaddr *)&addr, &len) == 0);
    *bound = ntohs(addr.sin_port);
  }
  return fd;
}

// A reusable loopback listener on `port` with a backlog of one.
static inline int test_loopback_listen(int port) {
  return test_loopback_bind(port, 1, NULL);
}

// A backlog-of-one listener on an OS-assigned port, written to *port.
static inline int test_loopback_listen_ephemeral(int *port) {
  return test_loopback_bind(0, 1, port);
}

// An OS-assigned loopback port that is free right now [CI-TEST-PORTS]. The
// probe socket is closed first, so the caller can hand the number to an API
// that cannot bind port 0 itself, or dial it back expecting a refusal.
static inline int test_free_loopback_port(void) {
  int probe;
  int fd = test_loopback_listen_ephemeral(&probe);
  close(fd);
  return probe;
}

// The port in `*slot`, probed with test_free_loopback_port the first time,
// so every test in a suite that caches it in one static slot agrees on it.
static inline int test_cached_port(int *slot) {
  if (*slot == 0) {
    *slot = test_free_loopback_port();
  }
  return *slot;
}

#endif
