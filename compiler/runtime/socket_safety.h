#ifndef OSPREY_SOCKET_SAFETY_H
#define OSPREY_SOCKET_SAFETY_H

#include <limits.h>

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
  if (!osp_socket_configure_no_sigpipe(socket_fd)) {
    return -1;
  }
  int chunk = length > (size_t)INT_MAX ? INT_MAX : (int)length;
  return (int)send(socket_fd, data, chunk, OSPREY_SOCKET_SEND_FLAGS);
}

#endif
