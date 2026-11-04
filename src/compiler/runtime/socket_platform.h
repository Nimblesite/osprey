#ifndef OSPREY_SOCKET_PLATFORM_H
#define OSPREY_SOCKET_PLATFORM_H

// Shared socket platform surface, including mobile targets without OpenSSL.
#include <errno.h>
#include <inttypes.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

// Sockets: Winsock2 on Windows (via the compat header), BSD sockets elsewhere.
#ifdef _WIN32
#include "osprey_win_compat.h"
#else
#include <arpa/inet.h>
#include <fcntl.h>
#include <netdb.h>
#include <netinet/in.h>
#include <sys/select.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <unistd.h>
// Winsock needs starting before the first socket call; BSD sockets do not.
static inline void osprey_wsa_init(void) {}
#endif

static inline uint64_t osp_monotonic_ms(void) {
#ifdef _WIN32
  return (uint64_t)GetTickCount64();
#else
  struct timespec now;
  return clock_gettime(CLOCK_MONOTONIC, &now) == 0
             ? (uint64_t)now.tv_sec * 1000U + (uint64_t)now.tv_nsec / 1000000U
             : 0;
#endif
}

#endif
