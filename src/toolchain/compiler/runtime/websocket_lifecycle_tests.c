// A WebSocket server's connections over time, against real loopback sockets, split
// from websocket_runtime_tests.c so each test TU stays under the repo size budget: the
// list holds the live ones and nothing else, however many have come and gone, and
// stopping ends them all before their state goes. [BUILTIN-WEBSOCKET-LIFECYCLE]
#include "http_shared.h"
#include "test_loopback.h"
#include "websocket_test_helpers.h"

#include <assert.h>
#include <stdatomic.h>

enum {
  WS_PAST_THE_OLD_LIMIT = 1100,
  // Waiting for the server's reader thread to notice a disconnect is
  // environment synchronization, not the behavior under test, so this budget
  // is a generous safety deadline (30 s) rather than a claim about latency:
  // a starved runner must not turn a scheduling stall into a broadcast
  // assertion failure. Only ordering matters.
  WS_POLLS = 3000,
  WS_POLL_US = 10000,
  WS_CHURN = 150,
  WS_BIG_FRAME = 4000
};

// One OS-assigned port for this TU's servers [CI-TEST-PORTS]: probed once,
// then bound by the runtime server, which cannot take port 0.
static int ws_life_port(void) {
  static int port = 0;
  return port ? port : (port = test_free_loopback_port());
}

static char *const ADDRESS = (char *)(uintptr_t) "127.0.0.1";
static char *const PATH = (char *)(uintptr_t) "/life";
static char *const PING = (char *)(uintptr_t) "ping";

static int64_t started_server(void) {
  int64_t server = websocket_create_server(ws_life_port(), ADDRESS, PATH);
  assert(server > 0 && websocket_server_listen(server) == 0);
  return server;
}

static int upgraded_client(void) { return ws_upgraded_client(ws_life_port()); }

static int64_t broadcast_until(int64_t server, int64_t recipients, int *frames_sent) {
  int64_t reached = -1;
  for (int i = 0; i < WS_POLLS && reached != recipients; i++) {
    reached = websocket_server_broadcast(server, PING);
    *frames_sent += reached > 0;
    if (reached != recipients) {
      usleep(WS_POLL_US);
    }
  }
  return reached;
}

static void expect_silence(int fd) {
  struct timeval brief = {0, 100000};
  assert(setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &brief, sizeof brief) == 0);
  char byte;
  assert(recv(fd, &byte, 1, 0) < 0);
}

// A closed connection stops counting, and its old descriptor number, which the next
// socket reuses, is never written to: a broadcast once reached strangers that way.
static void check_dead_connections_leave_the_list(int64_t server) {
  for (int round = 0; round < 10; round++) {
    int stays = upgraded_client();
    int leaves = upgraded_client();
    // Both clients received their welcome frames. The first broadcast reaches both.
    assert(websocket_server_broadcast(server, PING) == 2);
    char *departing_ping = ws_read_text_frame(leaves);
    assert(strcmp(departing_ping, "ping") == 0);
    free(departing_ping);
    close(leaves);
    int frames = 1;
    assert(broadcast_until(server, 1, &frames) == 1);
    int stranger = ws_raw_connect(ws_life_port());
    usleep(WS_POLL_US * 5);
    int later = 0;
    assert(broadcast_until(server, 1, &later) == 1 && later == 1);
    expect_silence(stranger);
    close(stranger);
    for (int i = 0; i < frames + later; i++) {
      char *ping = ws_read_text_frame(stays);
      assert(strcmp(ping, "ping") == 0);
      free(ping);
    }
    close(stays);
    int ignored = 0;
    assert(broadcast_until(server, 0, &ignored) == 0);
  }
}

// Far more connections over its life than the list holds at once.
static void check_lifetime_connections_are_unlimited(int64_t server) {
  int frames = 0;
  for (int i = 0; i < WS_PAST_THE_OLD_LIMIT; i++) {
    close(upgraded_client());
  }
  assert(broadcast_until(server, 0, &frames) == 0);
  int last = upgraded_client();
  assert(broadcast_until(server, 1, &frames) == 1);
  char *ping = ws_read_text_frame(last);
  assert(strcmp(ping, "ping") == 0);
  free(ping);
  close(last);
}

// A peer that stopped reading can't hold the server up. Once its socket is full the
// frame can't go, so it stops counting, and the others keep getting theirs.
static void check_stalled_peer_is_dropped(int64_t server) {
  static char big[WS_BIG_FRAME + 1];
  memset(big, 'x', WS_BIG_FRAME);
  int stalled = upgraded_client();
  int frames = 0;
  assert(broadcast_until(server, 1, &frames) == 1);
  int64_t reached = 1;
  for (int i = 0; i < 20000 && reached == 1; i++) {
    reached = websocket_server_broadcast(server, big);
  }
  assert(reached == 0);
  close(stalled);
  int next = upgraded_client();
  assert(broadcast_until(server, 1, &frames) == 1);
  close(next);
}

static atomic_bool churn_done;

static void *churn_connections(void *ignored) {
  (void)ignored;
  for (int i = 0; i < WS_CHURN; i++) {
    close(upgraded_client());
  }
  atomic_store(&churn_done, true);
  return NULL;
}

static void check_broadcast_during_churn(int64_t server) {
  pthread_t churner;
  atomic_store(&churn_done, false);
  assert(pthread_create(&churner, NULL, churn_connections, NULL) == 0);
  while (!atomic_load(&churn_done)) {
    assert(websocket_server_broadcast(server, PING) >= 0);
  }
  assert(pthread_join(churner, NULL) == 0);
  int frames = 0;
  assert(broadcast_until(server, 0, &frames) == 0);
}

// Stop ends every connection in every state before it frees what they read, then the
// same port serves again: the freed connection once closed someone else's descriptor.
static void check_stop_ends_every_connection(void) {
  for (int round = 0; round < 8; round++) {
    int64_t server = started_server();
    int live = upgraded_client();
    int other = upgraded_client();
    int silent = ws_raw_connect(ws_life_port());
    int partial = ws_raw_connect(ws_life_port());
    assert(send(partial, "GET /life", 9, 0) == 9);
    assert(websocket_stop_server(server) == 0);
    ws_expect_peer_close(live);
    ws_expect_peer_close(other);
    ws_expect_peer_close(silent);
    close(live);
    close(other);
    close(silent);
    close(partial);
    assert(websocket_server_broadcast(server, PING) == -2);
    assert(websocket_stop_server(server) == 0);
  }
}

// [BUILTIN-WEBSOCKET-FRAMING] Empty text frames own no unreleased payload.
static void check_empty_frame_payloads_are_released(int64_t server) {
  const unsigned char empty[] = {0x81, 0x00};
  for (int i = 0; i < 64; i++) {
    int fd = upgraded_client();
    assert(send(fd, (const char *)empty, sizeof empty, 0) == (ssize_t)sizeof empty);
    assert(shutdown(fd, SHUT_WR) == 0);
    ws_expect_peer_close(fd);
    close(fd);
  }
  int ignored = 0;
  assert(broadcast_until(server, 0, &ignored) == 0);
}

void test_websocket_lifecycle(void) {
  int64_t server = started_server();
  check_empty_frame_payloads_are_released(server);
  check_dead_connections_leave_the_list(server);
  check_lifetime_connections_are_unlimited(server);
  check_stalled_peer_is_dropped(server);
  check_broadcast_during_churn(server);
  assert(websocket_stop_server(server) == 0);
  check_stop_ends_every_connection();
}
