// Loopback sockets the HTTP, TLS and WebSocket C suites share: one address
// builder, one single-connection listener and one port block per process, so
// no suite re-spells them.
#ifndef OSPREY_TEST_LOOPBACK_H
#define OSPREY_TEST_LOOPBACK_H

#include "http_shared.h"

#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static inline struct sockaddr_in test_loopback_addr(int port) {
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof(addr));
  addr.sin_family = AF_INET;
  addr.sin_port = htons((uint16_t)port);
  addr.sin_addr.s_addr = inet_addr("127.0.0.1");
  return addr;
}

// [CI-TEST-PORTS] Every suite, and each suite's ASan twin, runs beside the
// others under `make -j`, so a fixed port is bound twice. Each process takes
// its own block of TEST_PORT_BLOCK ports below the ephemeral range, starting
// at a pid-derived block and keeping the first whose ports all bind. The base
// lives in the environment, not a per-file static, so every translation unit of
// one binary (an origin and its client) agrees on it.
enum {
  TEST_PORT_FIRST = 20000,
  TEST_PORT_BLOCK = 24,
  TEST_PORT_BLOCKS = 500,
};
#define TEST_PORT_ENV "OSP_TEST_PORT_BASE"

// Whether every port of the block at `base` binds on loopback right now.
static inline int test_port_block_free(int base) {
  for (int port = base; port < base + TEST_PORT_BLOCK; port++) {
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    assert(fd >= 0);
    struct sockaddr_in addr = test_loopback_addr(port);
    int bound = bind(fd, (struct sockaddr *)&addr, sizeof(addr)) == 0;
    close(fd);
    if (!bound) {
      return 0;
    }
  }
  return 1;
}

// The first port of this process's block, chosen once and kept in the
// environment.
static inline int test_port_base(void) {
  const char *kept = getenv(TEST_PORT_ENV);
  if (kept != NULL) {
    return atoi(kept);
  }
  int start = (int)(getpid() % TEST_PORT_BLOCKS);
  for (int step = 0; step < TEST_PORT_BLOCKS; step++) {
    int base = TEST_PORT_FIRST + ((start + step) % TEST_PORT_BLOCKS) * TEST_PORT_BLOCK;
    if (test_port_block_free(base)) {
      char text[16];
      snprintf(text, sizeof(text), "%d", base);
      int kept_now = setenv(TEST_PORT_ENV, text, 1);
      assert(kept_now == 0);
      (void)kept_now;
      return base;
    }
  }
  assert(!"no free loopback port block");
  return -1;
}

// Chosen before main, so no thread or forked child races to pick its own.
__attribute__((constructor)) static void test_port_choose(void) {
  (void)test_port_base();
}

// Port `offset` of this process's block; each listener a suite opens has its
// own offset.
static inline int test_port(int offset) {
  assert(offset >= 0 && offset < TEST_PORT_BLOCK);
  return test_port_base() + offset;
}

// "<scheme>://127.0.0.1:<port>" in a buffer the caller owns.
static inline char *test_loopback_url(char *into, size_t size, const char *scheme, int port) {
  int written = snprintf(into, size, "%s://127.0.0.1:%d", scheme, port);
  assert(written > 0 && (size_t)written < size);
  return into;
}

// A loopback URL in a buffer that lives as long as the enclosing block.
#define TEST_URL(scheme, port) test_loopback_url((char[64]){0}, 64, (scheme), (port))

// A reusable loopback listener on `port` with a backlog of one.
static inline int test_loopback_listen(int port) {
  int fd = socket(AF_INET, SOCK_STREAM, 0);
  assert(fd >= 0);
  int opt = 1;
  assert(setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, (const char *)&opt,
                    sizeof(opt)) == 0);
  struct sockaddr_in addr = test_loopback_addr(port);
  assert(bind(fd, (struct sockaddr *)&addr, sizeof(addr)) == 0);
  assert(listen(fd, 1) == 0);
  return fd;
}

// A loopback port the caller knows is closed: this binds one, reads the
// number back and closes it, so a refused-dial assertion targets a port no
// unrelated process was holding a moment ago instead of a fixed number any
// listener on the host can occupy. The kernel hands out only ports nothing
// else holds, so the port was provably shut the instant before the dial.
static inline int test_loopback_closed_port(void) {
  int fd = socket(AF_INET, SOCK_STREAM, 0);
  assert(fd >= 0);
  struct sockaddr_in addr = test_loopback_addr(0);
  assert(bind(fd, (struct sockaddr *)&addr, sizeof(addr)) == 0);
  struct sockaddr_in bound;
  socklen_t size = sizeof(bound);
  assert(getsockname(fd, (struct sockaddr *)&bound, &size) == 0);
  int port = ntohs(bound.sin_port);
  assert(close(fd) == 0);
  return port;
}

#endif
