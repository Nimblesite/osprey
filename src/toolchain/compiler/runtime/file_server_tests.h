// What the file server suites share: the server's entry points and the plain TCP
// client file_server_tests.c defines for every suite that dials it.
#ifndef FILE_SERVER_TESTS_H
#define FILE_SERVER_TESTS_H

#include "http_shared.h"

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

void run_file_server_tests(void);                                // file_server_tests.c
void run_file_server_link_tests(const char *folder, int64_t port); // file_server_link_tests.c

#endif
