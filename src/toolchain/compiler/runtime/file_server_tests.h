// What the file server suites share: the server's entry points, the plain TCP
// client file_server_tests.c defines for every suite that dials it, and the file
// helpers that read the same on POSIX and Windows, where the suite also runs.
#ifndef FILE_SERVER_TESTS_H
#define FILE_SERVER_TESTS_H

#include "socket_platform.h"
#include "socket_safety.h"

int64_t osprey_file_server_start(char *path, char *content_type, char *address);
int64_t osprey_file_server_stop(int64_t port);

typedef struct {
  int status;
  char headers[4096];
  unsigned char *body;
  size_t body_len;
} Reply;

// One request and what its reply must carry: the status, every header line in
// `headers`, and a body that is the literal `text`, or MEDIA bytes
// [start, start + len), or empty (len 0); len -1 leaves the body unchecked.
typedef struct {
  const char *request;
  int status;
  const char *headers[3];
  const char *text;
  int64_t start;
  int64_t len;
} Exchange;

// Sends one raw request and reads the whole reply until the server closes.
void fetch(int port, const char *request, Reply *reply);
void expect_exchanges(int64_t port, const Exchange *cases, size_t count);
// Writes `text` as the file `name` in `folder`.
void write_text(const char *folder, const char *name, const char *text);

// A new file `<temp>/<stem>.<unique>`, created exclusively: its path in `out` and its
// open descriptor, which fs_close_file closes (on Windows `close` is closesocket).
int fs_temp_file(char *out, size_t cap, const char *stem);
// A new, empty folder `<temp>/<stem>.<unique>`, its path in `out`.
void fs_temp_folder(char *out, size_t cap, const char *stem);
void fs_close_file(int fd);
void fs_make_folder(const char *path);

void run_file_server_link_tests(const char *folder, int64_t port); // file_server_link_tests.c

#endif
