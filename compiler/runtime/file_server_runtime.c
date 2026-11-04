// One file over HTTP, with byte ranges, for `extern fn` callers
// [FFI-FILE-SERVER].
//
// `httpListen` hands a handler the request and takes the body back as an
// Osprey string, which cannot carry a NUL byte, so a media file cannot be
// served through it. This unit serves exactly one file at the path `/media`
// from a background thread: GET and HEAD, `Range: bytes=a-b` answered with
// 206 and `Content-Range`, a range past the end with 416, any other path with
// 404 and any other method with 405. A player that seeks (a Chromecast does)
// opens several connections, so each accepted connection is its own thread.
//
// A folder is served too ([FFI-FILE-SERVER-FOLDER]): `/<name>` answers the
// file of that name in it, typed by its extension, and a name that does not
// exist yet is waited for, up to FS_WAIT seconds, because a renderer may
// still be writing it ([FFI-MEDIA-RENDER]). Names with a separator of any
// platform, a colon or a leading dot are refused, so nothing outside the folder
// is reachable.
//
// `osprey_file_server_start` binds port 0 on the given address and returns
// the port chosen; `osprey_file_server_stop` closes the listener, ends every
// connection and only then releases what they read ([FFI-FILE-SERVER-STOP]).

// System headers first: on Windows http_shared.h maps close() to closesocket(),
// which would rename the CRT's own close() declaration.
#include <errno.h>
#include <stdatomic.h>
#include <sys/stat.h>

#include "socket_platform.h"
#include "socket_safety.h"

enum { FS_TICK_MS = 100 }; // how soon a connection blocked on its peer notices a stop
enum { FS_SERVERS = 16, FS_CHUNK = 65536, FS_REQUEST = 8192, FS_WAIT = 120 };

typedef struct {
  int listen_fd;
  int port;
  bool running;
  atomic_bool stopping;
  pthread_t thread;
  char *path;
  char *content_type;
  int64_t size;
  bool folder;
  int connections; // the live ones, under fs_connections_mutex
} FileServer;

// One file a request resolved to: its path, type and size.
typedef struct {
  char path[1280];
  const char *content_type;
  int64_t size;
} Served;

typedef struct {
  FileServer *server;
  int fd;
} Connection;

static FileServer fs_servers[FS_SERVERS];
static pthread_mutex_t fs_mutex = PTHREAD_MUTEX_INITIALIZER;
static pthread_mutex_t fs_connections_mutex = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t fs_connection_left = PTHREAD_COND_INITIALIZER;

// Whether a socket call that failed is worth repeating: it was interrupted, or its
// tick ran out while the peer was silent, and the server isn't stopping.
static bool retry_socket(const Connection *connection) {
  int error = osp_socket_error();
  bool waiting = osp_socket_timed_out(error) || error == EINTR;
  return waiting && !atomic_load(&connection->server->stopping);
}

// Reads request bytes until the blank line that ends the headers, or the buffer
// fills. False when the peer stops first: half a request gets no answer. The text
// is NUL-terminated either way.
static bool read_request(const Connection *connection, char *out, size_t capacity) {
  size_t len = 0;
  out[0] = '\0';
  while (len + 1 < capacity && strstr(out, "\r\n\r\n") == NULL) {
    ssize_t n = recv(connection->fd, out + len, capacity - 1 - len, 0);
    if (n == 0 || (n < 0 && !retry_socket(connection))) {
      return false;
    }
    len += n > 0 ? (size_t)n : 0;
    out[len] = '\0';
  }
  return true;
}

// The first header line split into method and path; false if malformed.
static bool request_line(const char *request, char *method, size_t method_cap, char *path,
                         size_t path_cap) {
  const char *space = strchr(request, ' ');
  if (!space || (size_t)(space - request) >= method_cap) {
    return false;
  }
  memcpy(method, request, (size_t)(space - request));
  method[space - request] = '\0';
  const char *end = strpbrk(space + 1, " \r\n");
  if (!end || (size_t)(end - space - 1) >= path_cap) {
    return false;
  }
  memcpy(path, space + 1, (size_t)(end - space - 1));
  path[end - space - 1] = '\0';
  return true;
}

// Finds `Range: bytes=start-end` (end optional). Returns false when the
// request has no Range header; `*end` is -1 for an open range.
static bool range_header(const char *request, int64_t *start, int64_t *end) {
  const char *header = strstr(request, "\r\nRange: bytes=");
  if (!header) {
    return false;
  }
  const char *spec = header + strlen("\r\nRange: bytes=");
  char *after = NULL;
  long long first = strtoll(spec, &after, 10);
  if (after == spec || *after != '-' || first < 0) {
    return false;
  }
  char *tail = NULL;
  long long last = strtoll(after + 1, &tail, 10);
  *start = first;
  *end = tail == after + 1 ? -1 : last;
  return true;
}

// Sends all of it. False when the peer went or the server is stopping.
static bool send_all(const Connection *connection, const char *data, size_t left) {
  while (left > 0) {
    int n = osp_socket_send(connection->fd, data, left);
    if (n > 0) {
      data += n;
      left -= (size_t)n;
    } else if (!retry_socket(connection)) {
      return false;
    }
  }
  return true;
}

static void send_status(const Connection *connection, const char *status, const char *extra) {
  char head[256];
  snprintf(head, sizeof head, "HTTP/1.1 %s\r\nConnection: close\r\n%sContent-Length: 0\r\n\r\n",
           status, extra);
  (void)send_all(connection, head, strlen(head));
}

// Streams bytes [start, end] of the file in FS_CHUNK pieces, stopping at the first
// failed send (a player that seeks away closes its old connection) or a server stop.
static void send_body(const Connection *connection, const Served *served, int64_t start,
                      int64_t end) {
  FILE *file = fopen(served->path, "rb");
  if (!file || fseeko(file, (off_t)start, SEEK_SET) != 0) {
    if (file) {
      fclose(file);
    }
    return;
  }
  char chunk[FS_CHUNK];
  int64_t left = end - start + 1;
  while (left > 0 && !atomic_load(&connection->server->stopping)) {
    size_t want = left < FS_CHUNK ? (size_t)left : FS_CHUNK;
    size_t got = fread(chunk, 1, want, file);
    if (got == 0 || !send_all(connection, chunk, got)) {
      break;
    }
    left -= (int64_t)got;
  }
  fclose(file);
}

// Answers a GET or HEAD for the media: 200 for the whole file, 206 for a
// satisfiable range, 416 for one past the end. Any origin may read it: a Cast
// receiver is a web page that fetches an HLS playlist and its segments by XHR.
static void serve_media(const Connection *connection, const Served *server, const char *request,
                        bool head) {
  int64_t start = 0;
  int64_t end = server->size - 1;
  bool ranged = range_header(request, &start, &end);
  if (ranged && (end < 0 || end >= server->size)) {
    end = server->size - 1;
  }
  if (ranged && (start >= server->size || start > end)) {
    char extra[96];
    snprintf(extra, sizeof extra, "Content-Range: bytes */%lld\r\n", (long long)server->size);
    send_status(connection, "416 Range Not Satisfiable", extra);
    return;
  }
  char range_line[128] = "";
  if (ranged) {
    snprintf(range_line, sizeof range_line, "Content-Range: bytes %lld-%lld/%lld\r\n",
             (long long)start, (long long)end, (long long)server->size);
  }
  char headers[512];
  snprintf(headers, sizeof headers,
           "HTTP/1.1 %s\r\nContent-Type: %s\r\nAccept-Ranges: bytes\r\n"
           "Access-Control-Allow-Origin: *\r\n%sContent-Length: "
           "%lld\r\nConnection: close\r\n\r\n",
           ranged ? "206 Partial Content" : "200 OK", server->content_type, range_line,
           (long long)(end - start + 1));
  if (send_all(connection, headers, strlen(headers)) && !head) {
    send_body(connection, server, start, end);
  }
}

// The type a folder's file is served as, by its extension.
static const char *folder_type(const char *name) {
  const char *dot = strrchr(name, '.');
  if (dot && strcmp(dot, ".m3u8") == 0) {
    return "application/vnd.apple.mpegurl";
  }
  if (dot && strcmp(dot, ".ts") == 0) {
    return "video/mp2t";
  }
  return "application/octet-stream";
}

// Whether a folder's file can have this name. Every platform refuses the separators
// and colons of all of them: on Windows a backslash climbs out of the folder, and a
// colon names a drive or an alternate stream.
static bool is_plain_name(const char *name) {
  return name[0] != '\0' && name[0] != '.' && !strpbrk(name, "/\\:");
}

// Implements [FFI-FILE-SERVER-FOLDER]: never follow a folder entry outside its root.
static bool is_folder_link(const char *path) {
#ifdef _WIN32
  DWORD attributes = GetFileAttributesA(path);
  return attributes != INVALID_FILE_ATTRIBUTES &&
         (attributes & FILE_ATTRIBUTE_REPARSE_POINT) != 0;
#else
  struct stat info;
  return lstat(path, &info) == 0 && S_ISLNK(info.st_mode);
#endif
}

// Resolves a request path to the file it serves: the one file at `/media`, or
// a folder's `/<name>` once that file exists. False answers 404.
static bool resolve(const FileServer *server, const char *path, Served *served) {
  if (!server->folder) {
    snprintf(served->path, sizeof served->path, "%s", server->path);
    served->content_type = server->content_type;
    served->size = server->size;
    return strcmp(path, "/media") == 0;
  }
  const char *name = path + 1;
  if (path[0] != '/' || !is_plain_name(name)) {
    return false;
  }
  snprintf(served->path, sizeof served->path, "%s/%s", server->path, name);
  served->content_type = folder_type(name);
  struct stat info;
  for (int waited = 0; stat(served->path, &info) != 0; waited++) {
    if (waited >= FS_WAIT * 10 || atomic_load(&server->stopping)) {
      return false;
    }
    usleep(100000);
  }
  if (is_folder_link(served->path)) {
    return false;
  }
  served->size = (int64_t)info.st_size;
  return S_ISREG(info.st_mode);
}

static void connection_join(Connection *connection) {
  pthread_mutex_lock(&fs_connections_mutex);
  connection->server->connections++;
  pthread_mutex_unlock(&fs_connections_mutex);
}

// The server may be released as soon as the lock drops, so nothing reads it after this.
static void connection_leave(Connection *connection) {
  pthread_mutex_lock(&fs_connections_mutex);
  connection->server->connections--;
  pthread_cond_broadcast(&fs_connection_left);
  pthread_mutex_unlock(&fs_connections_mutex);
}

static void *connection_thread(void *arg) {
  Connection *connection = arg;
  char request[FS_REQUEST];
  char method[16];
  char path[1024];
  Served served;
  if (!read_request(connection, request, sizeof request)) {
    // nothing to answer
  } else if (!request_line(request, method, sizeof method, path, sizeof path)) {
    send_status(connection, "400 Bad Request", "");
  } else if (strcmp(method, "GET") != 0 && strcmp(method, "HEAD") != 0) {
    send_status(connection, "405 Method Not Allowed", "Allow: GET, HEAD\r\n");
  } else if (!resolve(connection->server, path, &served)) {
    send_status(connection, "404 Not Found", "");
  } else {
    serve_media(connection, &served, request, strcmp(method, "HEAD") == 0);
  }
  connection_leave(connection);
  close(connection->fd);
  free(connection);
  return NULL;
}

static void *accept_thread(void *arg) {
  FileServer *server = arg;
  for (;;) {
    int fd = accept(server->listen_fd, NULL, NULL);
    if (fd < 0 || atomic_load(&server->stopping)) {
      if (fd >= 0) {
        close(fd);
      }
      return NULL;
    }
    osp_socket_configure_no_sigpipe(fd);
    osp_socket_deadline(fd, FS_TICK_MS);
    Connection *connection = malloc(sizeof *connection);
    pthread_t thread;
    if (!connection) {
      close(fd);
      continue;
    }
    connection->server = server;
    connection->fd = fd;
    connection_join(connection);
    if (pthread_create(&thread, NULL, connection_thread, connection) != 0) {
      connection_leave(connection);
      close(fd);
      free(connection);
      continue;
    }
    pthread_detach(thread);
  }
}

// A listening socket on address:0, its chosen port written to *port.
static int listen_on(const char *address, int *port) {
  int fd = socket(AF_INET, SOCK_STREAM, 0);
  if (fd < 0) {
    return -1;
  }
  int one = 1;
  setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, (const char *)&one, sizeof one);
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof addr);
  addr.sin_family = AF_INET;
  addr.sin_port = 0;
  if (inet_pton(AF_INET, address, &addr.sin_addr) != 1 ||
      bind(fd, (struct sockaddr *)&addr, sizeof addr) != 0 || listen(fd, 16) != 0) {
    close(fd);
    return -1;
  }
  socklen_t len = sizeof addr;
  if (getsockname(fd, (struct sockaddr *)&addr, &len) != 0) {
    close(fd);
    return -1;
  }
  *port = ntohs(addr.sin_port);
  return fd;
}

static FileServer *free_server(void) {
  for (int i = 0; i < FS_SERVERS; i++) {
    if (!fs_servers[i].running) {
      return &fs_servers[i];
    }
  }
  return NULL;
}

static void release_server(FileServer *server) {
  free(server->path);
  free(server->content_type);
  memset(server, 0, sizeof *server);
  server->listen_fd = -1;
}

// osprey_file_server_start serves `path` as `content_type` on a fresh port of
// `address` and returns that port; a folder `path` serves its files by name. Negative reasons: -1 bad argument, -2 the
// file cannot be read, -3 no server slot, -4 the address cannot be bound,
// -5 the accept thread did not start.
int64_t osprey_file_server_start(char *path, char *content_type, char *address) {
  struct stat info;
  if (!path || !content_type || !address) {
    return -1;
  }
  if (stat(path, &info) != 0 || !(S_ISREG(info.st_mode) || S_ISDIR(info.st_mode))) {
    return -2;
  }
  osprey_wsa_init();
  pthread_mutex_lock(&fs_mutex);
  FileServer *server = free_server();
  if (!server) {
    pthread_mutex_unlock(&fs_mutex);
    return -3;
  }
  memset(server, 0, sizeof *server);
  server->path = strdup(path);
  server->content_type = strdup(content_type);
  server->size = (int64_t)info.st_size;
  server->folder = S_ISDIR(info.st_mode);
  server->listen_fd = listen_on(address, &server->port);
  if (server->listen_fd < 0 || !server->path || !server->content_type) {
    release_server(server);
    pthread_mutex_unlock(&fs_mutex);
    return -4;
  }
  server->running = true;
  if (pthread_create(&server->thread, NULL, accept_thread, server) != 0) {
    close(server->listen_fd);
    release_server(server);
    pthread_mutex_unlock(&fs_mutex);
    return -5;
  }
  pthread_mutex_unlock(&fs_mutex);
  return server->port;
}

// Dials the listener once so a blocked accept returns and sees `stopping`:
// closing the descriptor from another thread does not wake it everywhere.
static void wake_listener(const FileServer *server) {
  int fd = socket(AF_INET, SOCK_STREAM, 0);
  if (fd < 0) {
    return;
  }
  struct sockaddr_in addr;
  socklen_t len = sizeof addr;
  if (getsockname(server->listen_fd, (struct sockaddr *)&addr, &len) == 0) {
    connect(fd, (struct sockaddr *)&addr, sizeof addr);
  }
  close(fd);
}

// Returns once no connection is left to read the server. Each notices `stopping`
// within a tick, however its peer behaves, and the accept thread has gone, so none
// can join. Shutting their sockets instead wouldn't do: on macOS that leaves another
// thread's blocked send blocked.
static void end_connections(FileServer *server) {
  pthread_mutex_lock(&fs_connections_mutex);
  while (server->connections > 0) {
    pthread_cond_wait(&fs_connection_left, &fs_connections_mutex);
  }
  pthread_mutex_unlock(&fs_connections_mutex);
}

// osprey_file_server_stop closes the listener on `port` and waits for the accept
// thread and every connection. 0, or -1 when no running server has that port.
int64_t osprey_file_server_stop(int64_t port) {
  pthread_mutex_lock(&fs_mutex);
  FileServer *server = NULL;
  for (int i = 0; i < FS_SERVERS && !server; i++) {
    if (fs_servers[i].running && fs_servers[i].port == port) {
      server = &fs_servers[i];
    }
  }
  if (!server) {
    pthread_mutex_unlock(&fs_mutex);
    return -1;
  }
  atomic_store(&server->stopping, true);
  wake_listener(server);
  pthread_join(server->thread, NULL);
  close(server->listen_fd);
  end_connections(server);
  release_server(server);
  pthread_mutex_unlock(&fs_mutex);
  return 0;
}
