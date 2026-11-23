// The file server, end to end against real loopback sockets with a plain TCP client.
// This suite OWNS file_server_runtime.c. Windows CI runs it too, so it reaches files
// only through file_server_test_files.c.
// [FFI-FILE-SERVER] 200/206/416/404/405 and byte-exact bodies
// [FFI-FILE-SERVER-FOLDER] files by name, and nothing outside the folder
// [FFI-FILE-SERVER-STOP] stopping ends every connection before its state goes
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <sys/stat.h>
#ifdef _WIN32
#include <io.h>
#endif

// Last: on Windows it maps close() to closesocket(), which would rename the CRT's
// own close() in any system header after it.
#include "file_server_tests.h"

enum { MEDIA_BYTES = 200000 };

static char LOOPBACK[] = "127.0.0.1";

static char media_path[256];

static unsigned char media_byte(int64_t i) { return (unsigned char)((i * 7 + i / 251) & 255); }

static void write_media(void) {
  FILE *file = fdopen(fs_temp_file(media_path, sizeof media_path, "osprey-net-tests"), "wb");
  assert(file != NULL);
  for (int64_t i = 0; i < MEDIA_BYTES; i++) {
    unsigned char b = media_byte(i);
    assert(fwrite(&b, 1, 1, file) == 1);
  }
  fclose(file);
}

// Starts a server on `path`, serving every file as `type`, on loopback.
static int64_t start_server(char *path, char *type) {
  int64_t port = osprey_file_server_start(path, type, LOOPBACK);
  assert(port > 0);
  return port;
}

// Connects the socket `fd` to the server on `port`.
static int connect_loopback(int fd, int64_t port) {
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof addr);
  addr.sin_family = AF_INET;
  addr.sin_port = htons((uint16_t)port);
  addr.sin_addr.s_addr = inet_addr(LOOPBACK);
  assert(connect(fd, (struct sockaddr *)&addr, sizeof addr) == 0);
  return fd;
}

// A fresh socket, not yet connected.
static int open_socket(void) {
  int fd = socket(AF_INET, SOCK_STREAM, 0);
  assert(fd >= 0);
  return fd;
}

// A plain connection to the server on `port`.
static int dial_loopback(int64_t port) {
  return connect_loopback(open_socket(), port);
}

// A connection with `receive_buffer` bytes of socket buffer and a 5s deadline.
// Both options go on before `connect`: Linux fixes the receive window at the
// handshake, so a buffer shrunk afterwards never makes the server's send block.
static int dial_with_deadline(int64_t port, int receive_buffer) {
  int fd = open_socket();
  osp_socket_deadline(fd, 5000);
  if (receive_buffer > 0) {
    assert(setsockopt(fd, SOL_SOCKET, SO_RCVBUF, (const char *)&receive_buffer, sizeof receive_buffer) == 0);
  }
  return connect_loopback(fd, port);
}

// Sends one raw request and reads the whole reply until the server closes.
void fetch(int port, const char *request, Reply *reply) {
  int fd = dial_loopback(port);
  assert(send(fd, request, strlen(request), 0) == (ssize_t)strlen(request));
  size_t cap = MEDIA_BYTES + 8192;
  unsigned char *all = malloc(cap);
  size_t len = 0;
  for (;;) {
    ssize_t n = recv(fd, (char *)all + len, cap - len, 0);
    if (n <= 0) {
      break;
    }
    len += (size_t)n;
  }
  close(fd);
  all[len] = '\0';
  char *blank = strstr((char *)all, "\r\n\r\n");
  assert(blank != NULL);
  size_t header_len = (size_t)(blank + 4 - (char *)all);
  memcpy(reply->headers, all, header_len);
  reply->headers[header_len] = '\0';
  reply->status = atoi((char *)all + 9);
  reply->body_len = len - header_len;
  reply->body = malloc(reply->body_len + 1);
  memcpy(reply->body, all + header_len, reply->body_len);
  free(all);
}

static void assert_body_is(const Reply *reply, int64_t start, int64_t count) {
  assert(reply->body_len == (size_t)count);
  for (int64_t i = 0; i < count; i++) {
    assert(reply->body[i] == media_byte(start + i));
  }
}

void expect_exchanges(int64_t port, const Exchange *cases, size_t count) {
  for (size_t i = 0; i < count; i++) {
    const Exchange *e = &cases[i];
    Reply reply;
    fetch((int)port, e->request, &reply);
    assert(reply.status == e->status);
    for (int h = 0; h < 3 && e->headers[h] != NULL; h++) {
      assert(strstr(reply.headers, e->headers[h]) != NULL);
    }
    if (e->text != NULL) {
      size_t n = strlen(e->text);
      assert(reply.body_len == n && memcmp(reply.body, e->text, n) == 0);
    } else if (e->len == 0) {
      assert(reply.body_len == 0);
    } else if (e->len > 0) {
      assert_body_is(&reply, e->start, e->len);
    }
    free(reply.body);
  }
}

static void test_file_server_whole_and_ranges(void) {
  char type[] = "audio/mpeg";
  int64_t port = start_server(media_path, type);
  // An open range runs to the end; an end past the file is clamped.
  static const Exchange cases[] = {
      {"GET /media HTTP/1.1\r\nHost: x\r\n\r\n", 200,
       {"Content-Type: audio/mpeg\r\n", "Accept-Ranges: bytes\r\n", "Content-Length: 200000\r\n"}, NULL, 0, MEDIA_BYTES},
      {"GET /media HTTP/1.1\r\nHost: x\r\nRange: bytes=100-199\r\n\r\n", 206,
       {"Content-Range: bytes 100-199/200000\r\n", "Content-Length: 100\r\n"}, NULL, 100, 100},
      {"GET /media HTTP/1.1\r\nRange: bytes=199990-\r\n\r\n", 206,
       {"Content-Range: bytes 199990-199999/200000\r\n"}, NULL, 199990, 10},
      {"GET /media HTTP/1.1\r\nRange: bytes=199000-999999\r\n\r\n", 206, {NULL}, NULL, 199000, 1000},
  };
  expect_exchanges(port, cases, sizeof cases / sizeof cases[0]);
  assert(osprey_file_server_stop(port) == 0);
}

static void test_file_server_refusals(void) {
  char type[] = "audio/mpeg";
  int64_t port = start_server(media_path, type);
  // A Range that is not `bytes=start-` is ignored: the whole file is sent.
  static const Exchange cases[] = {
      {"GET /media HTTP/1.1\r\nRange: bytes=300000-\r\n\r\n", 416, {"Content-Range: bytes */200000\r\n"}, NULL, 0, 0},
      {"GET /media HTTP/1.1\r\nRange: bytes=50-10\r\n\r\n", 416, {NULL}, NULL, 0, -1},
      {"GET /media HTTP/1.1\r\nRange: bytes=x-\r\n\r\n", 200, {NULL}, NULL, 0, MEDIA_BYTES},
      {"HEAD /media HTTP/1.1\r\n\r\n", 200, {"Content-Length: 200000\r\n"}, NULL, 0, 0},
      {"GET /other HTTP/1.1\r\n\r\n", 404, {NULL}, NULL, 0, -1},
      {"POST /media HTTP/1.1\r\n\r\n", 405, {"Allow: GET, HEAD\r\n"}, NULL, 0, -1},
      {"garbage\r\n\r\n", 400, {NULL}, NULL, 0, -1},
  };
  expect_exchanges(port, cases, sizeof cases / sizeof cases[0]);
  assert(osprey_file_server_stop(port) == 0);
  assert(osprey_file_server_stop(port) == -1);
}

static void test_file_server_start_failures(void) {
  char type[] = "audio/mpeg";
  char missing[] = "/tmp/osprey-net-tests-missing.bin";
  char device[] = "/dev/null";
  char bad_address[] = "not-an-address";
  assert(osprey_file_server_start(NULL, type, LOOPBACK) == -1);
  assert(osprey_file_server_start(media_path, NULL, LOOPBACK) == -1);
  assert(osprey_file_server_start(media_path, type, NULL) == -1);
  assert(osprey_file_server_start(missing, type, LOOPBACK) == -2);
  assert(osprey_file_server_start(device, type, LOOPBACK) == -2);
  assert(osprey_file_server_start(media_path, type, bad_address) == -4);
  int64_t ports[16];
  for (int i = 0; i < 16; i++) {
    ports[i] = osprey_file_server_start(media_path, type, LOOPBACK);
    assert(ports[i] > 0);
  }
  assert(osprey_file_server_start(media_path, type, LOOPBACK) == -3);
  for (int i = 0; i < 16; i++) {
    assert(osprey_file_server_stop(ports[i]) == 0);
  }
}

static void test_file_server_closes_with_the_player(void) {
  // A player that drops the connection mid-body must not take the server down.
  char type[] = "audio/mpeg";
  int64_t port = start_server(media_path, type);
  int fd = dial_with_deadline(port, 0);
  const char *request = "GET /media HTTP/1.1\r\n\r\n";
  assert(send(fd, request, strlen(request), 0) > 0);
  char first[16];
  assert(recv(fd, first, sizeof first, 0) > 0);
  close(fd);
  Reply reply;
  fetch((int)port, "GET /media HTTP/1.1\r\nRange: bytes=0-0\r\n\r\n", &reply);
  assert(reply.status == 206);
  assert_body_is(&reply, 0, 1);
  free(reply.body);
  assert(osprey_file_server_stop(port) == 0);
}

// [FFI-FILE-SERVER-FOLDER] A folder serves its files by name, typed by
// extension; a name that is not there yet is waited for until it appears or
// the server stops; names that could leave the folder are refused at once.
typedef struct {
  int port;
  const char *request;
  Reply reply;
} PendingFetch;

static void *fetch_thread(void *arg) {
  PendingFetch *pending = arg;
  fetch(pending->port, pending->request, &pending->reply);
  return NULL;
}

void write_text(const char *folder, const char *name, const char *text) {
  char path[512];
  snprintf(path, sizeof path, "%s/%s", folder, name);
  FILE *file = fopen(path, "w");
  assert(file != NULL);
  assert(fputs(text, file) >= 0);
  fclose(file);
}

// Folder tests only exercise the name check, so one ignored type serves every file.
static int64_t start_folder_server(char *folder) {
  char type[] = "ignored/for-folders";
  return start_server(folder, type);
}

// Unlinks every one of `names` under `folder`; the caller removes the folder.
static void unlink_in_folder(const char *folder, const char *const *names, size_t count) {
  for (size_t i = 0; i < count; i++) {
    char path[512];
    snprintf(path, sizeof path, "%s/%s", folder, names[i]);
    assert(unlink(path) == 0);
  }
}

static void test_file_server_folder(void) {
  char folder[512];
  fs_temp_folder(folder, sizeof folder, "osprey-net-folder");
  write_text(folder, "media.m3u8", "#EXTM3U\n");
  write_text(folder, "seg00000.ts", "segment-zero");
  write_text(folder, "cover.jpg", "jpeg");
  int64_t port = start_folder_server(folder);
  // A Cast receiver is a web page: it fetches HLS by XHR, which needs CORS.
  static const Exchange served[] = {
      {"GET /media.m3u8 HTTP/1.1\r\n\r\n", 200,
       {"Content-Type: application/vnd.apple.mpegurl\r\n", "Access-Control-Allow-Origin: *\r\n"}, "#EXTM3U\n", 0, 0},
      {"GET /seg00000.ts HTTP/1.1\r\nRange: bytes=8-\r\n\r\n", 206,
       {"Content-Type: video/mp2t\r\n", "Access-Control-Allow-Origin: *\r\n"}, "zero", 0, 0},
      {"HEAD /cover.jpg HTTP/1.1\r\n\r\n", 200,
       {"Content-Type: application/octet-stream\r\n", "Content-Length: 4\r\n"}, NULL, 0, 0},
      {"GET / HTTP/1.1\r\n\r\n", 404, {NULL}, NULL, 0, -1},
      {"GET /.hidden HTTP/1.1\r\n\r\n", 404, {NULL}, NULL, 0, -1},
      {"GET /../etc/passwd HTTP/1.1\r\n\r\n", 404, {NULL}, NULL, 0, -1},
      {"GET /sub/seg.ts HTTP/1.1\r\n\r\n", 404, {NULL}, NULL, 0, -1},
  };
  expect_exchanges(port, served, sizeof served / sizeof served[0]);
  // A segment the renderer has not written yet is served once it appears.
  PendingFetch late = {(int)port, "GET /seg00001.ts HTTP/1.1\r\n\r\n", {0}};
  pthread_t thread;
  assert(pthread_create(&thread, NULL, fetch_thread, &late) == 0);
  usleep(300000);
  // As the renderer does, so the waiting request can't see it half written.
  write_text(folder, ".partial", "segment-one");
  char partial[512];
  char finished[512];
  snprintf(partial, sizeof partial, "%s/.partial", folder);
  snprintf(finished, sizeof finished, "%s/seg00001.ts", folder);
  assert(rename(partial, finished) == 0);
  assert(pthread_join(thread, NULL) == 0);
  assert(late.reply.status == 200);
  assert(late.reply.body_len == 11 && memcmp(late.reply.body, "segment-one", 11) == 0);
  free(late.reply.body);
  // Stopping the server releases a request still waiting for its file.
  PendingFetch never = {(int)port, "GET /seg00009.ts HTTP/1.1\r\n\r\n", {0}};
  assert(pthread_create(&thread, NULL, fetch_thread, &never) == 0);
  usleep(300000);
  assert(osprey_file_server_stop(port) == 0);
  assert(pthread_join(thread, NULL) == 0);
  assert(never.reply.status == 404);
  free(never.reply.body);
  const char *names[] = {"media.m3u8", "seg00000.ts", "seg00001.ts", "cover.jpg"};
  unlink_in_folder(folder, names, sizeof names / sizeof names[0]);
  assert(rmdir(folder) == 0);
}

static const char *const HOSTILE[] = {"a\\b", "\\a", "a\\", "..\\a", "\\..\\..\\a", "C:a", "a:b", "a::$DATA", ":", "\\"};
#ifdef _WIN32
// Windows cannot hold these names, so it holds what they reach: `a::$DATA` is a's
// data stream and `a\` is a itself, so a real `a` is the bait the check must guard.
static const char *const BAIT[] = {"a"};
#else
// These files really exist in the folder, so only the name check can refuse them.
#define BAIT HOSTILE
#endif

// [FFI-FILE-SERVER-FOLDER] Every separator any platform honours is refused, and so
// is a drive or stream colon: on Windows "\..\..\secret" leaves the folder.
static void test_file_server_refuses_every_separator(void) {
  char folder[512];
  fs_temp_folder(folder, sizeof folder, "osprey-net-names");
  size_t count = sizeof HOSTILE / sizeof HOSTILE[0];
  size_t baits = sizeof BAIT / sizeof BAIT[0];
  for (size_t i = 0; i < baits; i++) {
    write_text(folder, BAIT[i], "must not be served");
  }
  write_text(folder, "plain.txt", "ok");
  int64_t port = start_folder_server(folder);
  for (size_t i = 0; i < count; i++) {
    char get[128], head[128];
    snprintf(get, sizeof get, "GET /%s HTTP/1.1\r\n\r\n", HOSTILE[i]);
    snprintf(head, sizeof head, "HEAD /%s HTTP/1.1\r\n\r\n", HOSTILE[i]);
    const Exchange refused[] = {{get, 404, {NULL}, NULL, 0, 0}, {head, 404, {NULL}, NULL, 0, -1}};
    expect_exchanges(port, refused, 2);
  }
  static const Exchange plain_served[] = {{"GET /plain.txt HTTP/1.1\r\n\r\n", 200, {NULL}, "ok", 0, 0}};
  expect_exchanges(port, plain_served, 1);
  run_file_server_link_tests(folder, port);
  assert(osprey_file_server_stop(port) == 0);
  unlink_in_folder(folder, BAIT, baits);
  const char *plain[] = {"plain.txt"};
  unlink_in_folder(folder, plain, 1);
  assert(rmdir(folder) == 0);
}

// Bytes the peer sends before closing; a read that times out fails the test.
static size_t drain_until_closed(int fd) {
  static char sink[65536];
  size_t total = 0;
  for (;;) {
    ssize_t n = recv(fd, sink, sizeof sink, 0);
    assert(n >= 0 || fs_peer_reset());
    if (n <= 0) {
      return total;
    }
    total += (size_t)n;
  }
}

// Dials `port` with `receive_buffer` bytes of socket buffer, sends `request`,
// lets the server pick it up for `settle_micros`, then stops it: the stop tests
// read a client left mid-request or mid-send across the stop.
static int send_then_stop(int64_t port, int receive_buffer, const char *request,
                          unsigned int settle_micros) {
  int fd = dial_with_deadline(port, receive_buffer);
  assert(send(fd, request, strlen(request), 0) > 0);
  usleep(settle_micros);
  assert(osprey_file_server_stop(port) == 0);
  return fd;
}

// [FFI-FILE-SERVER-STOP] A connection still reading its request ends with the stop,
// and never sees the slot's next server: finishing the request afterwards used to
// serve the new session's file to the old peer, off a server freed under it.
static void test_file_server_stop_ends_an_idle_connection(void) {
  char type[] = "audio/mpeg";
  int64_t port = start_server(media_path, type);
  int fd = send_then_stop(port, 0, "GET /media HTTP/1.1\r\nX-A: 1\r\n", 100000);
  int64_t next = start_server(media_path, type);
  (void)!send(fd, "\r\n", 2, 0);
  assert(drain_until_closed(fd) == 0);
  close(fd);
  assert(osprey_file_server_stop(next) == 0);
}

// [FFI-FILE-SERVER-STOP] A player that stopped reading can't hold the stop up or keep
// the connection past it, though its send is blocked on a full socket.
static void test_file_server_stop_ends_a_blocked_send(void) {
  enum { HUGE_BYTES = 64 * 1024 * 1024 };
  char huge[512];
  int file = fs_temp_file(huge, sizeof huge, "osprey-net-huge");
#ifdef _WIN32
  assert(_chsize_s(file, HUGE_BYTES) == 0);
#else
  assert(ftruncate(file, HUGE_BYTES) == 0);
#endif
  fs_close_file(file);
  char type[] = "video/mp4";
  int64_t port = start_server(huge, type);
  int fd = send_then_stop(port, 4096, "GET /media HTTP/1.1\r\n\r\n", 200000);
  size_t delivered = drain_until_closed(fd);
  assert(delivered < HUGE_BYTES);
  close(fd);
  assert(unlink(huge) == 0);
}

// Many stops in a row, each with connections in every state, on one reused slot.
static void test_file_server_stop_repeatedly(void) {
  char type[] = "audio/mpeg";
  for (int round = 0; round < 25; round++) {
    int64_t port = start_server(media_path, type);
    int silent = dial_with_deadline(port, 0);
    int partial = dial_with_deadline(port, 0);
    int fetching = dial_with_deadline(port, 4096);
    assert(send(partial, "GET /me", 7, 0) == 7);
    const char *request = "GET /media HTTP/1.1\r\n\r\n";
    assert(send(fetching, request, strlen(request), 0) > 0);
    assert(osprey_file_server_stop(port) == 0);
    assert(drain_until_closed(silent) == 0);
    assert(drain_until_closed(partial) == 0);
    (void)drain_until_closed(fetching);
    close(silent);
    close(partial);
    close(fetching);
  }
}

int main(void) {
#ifndef _WIN32
  signal(SIGPIPE, SIG_IGN); // a client writing to a stopped server must fail, not die
#endif
  osprey_wsa_init();
  write_media();
  test_file_server_whole_and_ranges();
  test_file_server_refusals();
  test_file_server_start_failures();
  test_file_server_closes_with_the_player();
  test_file_server_folder();
  test_file_server_refuses_every_separator();
  test_file_server_stop_ends_an_idle_connection();
  test_file_server_stop_ends_a_blocked_send();
  test_file_server_stop_repeatedly();
  unlink(media_path);
  printf("✅ file server tests passed\n");
  return 0;
}
