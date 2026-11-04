#ifndef OSPREY_SOCKET_SAFETY_H
#define OSPREY_SOCKET_SAFETY_H

#include <errno.h>
#include <limits.h>
#include <stdbool.h>
#include <stdint.h>

#ifdef MSG_NOSIGNAL
#define OSPREY_SOCKET_SEND_FLAGS MSG_NOSIGNAL
#else
#define OSPREY_SOCKET_SEND_FLAGS 0
#endif

static inline bool osp_socket_configure_no_sigpipe(int socket_fd) {
#ifdef SO_NOSIGPIPE
  int enabled = 1;
  return setsockopt(socket_fd, SOL_SOCKET, SO_NOSIGPIPE, (const char *)&enabled,
                    sizeof(enabled)) == 0;
#elif defined(MSG_NOSIGNAL) || defined(_WIN32)
  (void)socket_fd;
  return true;
#else
  (void)socket_fd;
  return false;
#endif
}

static inline int osp_socket_send(int socket_fd, const char *data, size_t length) {
#if !defined(MSG_NOSIGNAL) && !defined(_WIN32)
  if (!osp_socket_configure_no_sigpipe(socket_fd)) {
    return -1;
  }
#endif
  int chunk = length > (size_t)INT_MAX ? INT_MAX : (int)length;
  return (int)send(socket_fd, data, chunk, OSPREY_SOCKET_SEND_FLAGS);
}

// Bounds every blocking read and write on `socket_fd` to `timeout_ms`.
// Winsock takes the deadline as a DWORD of milliseconds; handing it a timeval
// makes it read tv_sec as milliseconds, so a 30 s deadline became 30 ms.
static inline void osp_socket_deadline(int socket_fd, int64_t timeout_ms) {
#ifdef _WIN32
  DWORD ms = (DWORD)timeout_ms;
  setsockopt(socket_fd, SOL_SOCKET, SO_RCVTIMEO, (const char *)&ms, sizeof ms);
  setsockopt(socket_fd, SOL_SOCKET, SO_SNDTIMEO, (const char *)&ms, sizeof ms);
#else
  struct timeval tv;
  tv.tv_sec = (time_t)(timeout_ms / 1000);
  tv.tv_usec = (int)((timeout_ms % 1000) * 1000);
  setsockopt(socket_fd, SOL_SOCKET, SO_RCVTIMEO, (const char *)&tv, sizeof tv);
  setsockopt(socket_fd, SOL_SOCKET, SO_SNDTIMEO, (const char *)&tv, sizeof tv);
#endif
}

// The last socket error: Winsock reports through WSAGetLastError, not errno.
static inline int osp_socket_error(void) {
#ifdef _WIN32
  return WSAGetLastError();
#else
  return errno;
#endif
}

static inline void osp_socket_error_clear(void) {
#ifdef _WIN32
  WSASetLastError(0);
#else
  errno = 0;
#endif
}

// True when `error` (from osp_socket_error) means a socket deadline passed.
static inline bool osp_socket_timed_out(int error) {
#ifdef _WIN32
  return error == WSAETIMEDOUT || error == WSAEWOULDBLOCK;
#else
  return error == EAGAIN || error == EWOULDBLOCK;
#endif
}

// Resolves `host` (a name or dotted address) to one IPv4 address. getaddrinfo
// is reentrant where gethostbyname returns one static record per process, so
// two fibers dialing at once no longer read each other's answer. Asking for
// AF_INET is what bounds the copy: a resolver that answered with a 16-byte
// address used to be copied into this 4-byte field.
static inline bool osp_socket_resolve_ipv4(const char *host,
                                           struct in_addr *address) {
  if (!host) {
    return false;
  }
  struct addrinfo hints;
  memset(&hints, 0, sizeof hints);
  hints.ai_family = AF_INET;
  hints.ai_socktype = SOCK_STREAM;
  struct addrinfo *found = NULL;
  if (getaddrinfo(host, NULL, &hints, &found) != 0 || !found) {
    return false;
  }
  struct sockaddr_in resolved; // copied out: ai_addr is only byte-aligned
  memcpy(&resolved, found->ai_addr, sizeof resolved);
  *address = resolved.sin_addr;
  freeaddrinfo(found);
  return true;
}

// Whether a socket error means the peer went away without closing, as a device
// that loses power or a process that is killed does.
static inline bool osp_socket_reset(int error) {
#ifdef _WIN32
  return error == WSAECONNRESET || error == WSAECONNABORTED;
#else
  return error == ECONNRESET || error == ECONNABORTED;
#endif
}

#endif
