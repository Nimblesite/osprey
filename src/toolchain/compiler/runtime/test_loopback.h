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

#endif
