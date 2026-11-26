#include "http_server_pool.h"
#include "memory_parked_wait.h"
#include "socket_platform.h"

#ifdef _WIN32
#define http_poll WSAPoll
#else
#include <poll.h>
#define http_poll poll
#endif

// The poller and its workers [HTTP-SERVER-CONNECTIONS].


#define POOL_WORKERS_MIN 2
// Osprey heap operations share one lock across threads, so more workers than
// this only queue on it; four overlap the I/O of slow clients.
#define POOL_WORKERS_MAX 4
#define POOL_QUEUE 1024U
#define POOL_IDLE_MAX 1024U
#define POOL_POLL_MS 100

typedef struct HttpPool HttpPool;

typedef struct {
  HttpPool *pool;
  int index;
} WorkerSeat;

typedef struct {
  int fd;
  uint64_t since_ms;
} IdleConnection;

struct HttpPool {
  HttpServer *server;
  pthread_mutex_t mutex;
  // Signals both a queued connection and a finished handler; each waiter
  // rechecks its own condition, so every wake is a broadcast.
  pthread_cond_t work;
  // One handler runs at a time; the others wait, parked, for it to finish.
  bool handling;
  bool stopping;
  // Connections with a request waiting, oldest first; workers take them.
  int queue[POOL_QUEUE];
  size_t queue_head;
  size_t queue_count;
  // Connections a worker finished and kept open; the poller adopts them.
  int returned[POOL_QUEUE];
  size_t returned_count;
  int serving[POOL_WORKERS_MAX];
  pthread_t threads[POOL_WORKERS_MAX];
  bool thread_known[POOL_WORKERS_MAX];
  Fiber *fibers[POOL_WORKERS_MAX]; // a hold on each worker until it is joined
  WorkerSeat seats[POOL_WORKERS_MAX];
  int workers;
  int wake[2]; // a worker writes [1] when it returns a connection
  // Owned by the poller alone.
  IdleConnection idle[POOL_IDLE_MAX];
  size_t idle_count;
};

static int worker_count(void) {
#ifdef _WIN32
  long online = 8;
#else
  long online = sysconf(_SC_NPROCESSORS_ONLN);
#endif
  return online < POOL_WORKERS_MIN   ? POOL_WORKERS_MIN
         : online > POOL_WORKERS_MAX ? POOL_WORKERS_MAX
                                     : (int)online;
}

// Close both ends; an end that never opened is -1, which close refuses.
static void close_wake_pair(int pair[2]) {
  (void)close(pair[0]);
  (void)close(pair[1]);
  pair[0] = pair[1] = -1;
}

// A connected loopback pair: the one wake signal every platform has.
static bool open_wake_pair(int pair[2]) {
  pair[0] = pair[1] = -1;
  int listener = socket(AF_INET, SOCK_STREAM, 0);
  struct sockaddr_in address;
  memset(&address, 0, sizeof(address));
  address.sin_family = AF_INET;
  address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
  socklen_t length = sizeof(address);
  bool ready = listener >= 0 &&
               bind(listener, (struct sockaddr *)&address, sizeof(address)) == 0 &&
               listen(listener, 1) == 0 &&
               getsockname(listener, (struct sockaddr *)&address, &length) == 0;
  // Plain assignments: on Windows these return an unsigned SOCKET.
  if (ready) {
    pair[1] = socket(AF_INET, SOCK_STREAM, 0);
  }
  ready = pair[1] >= 0 &&
          connect(pair[1], (struct sockaddr *)&address, sizeof(address)) == 0;
  if (ready) {
    pair[0] = accept(listener, NULL, NULL);
  }
  if (listener >= 0) {
    close(listener);
  }
  bool paired = pair[0] >= 0 && osp_socket_configure_no_sigpipe(pair[1]);
  if (!paired) {
    close_wake_pair(pair);
  }
  return paired;
}

static void wake_poller(HttpPool *pool) {
  char signal = 1;
  (void)send(pool->wake[1], &signal, 1, HTTP_SEND_FLAGS);
}

// A connection either closes (when it is not wanted) or joins the queue.
static void enqueue(HttpPool *pool, int fd) {
  pthread_mutex_lock(&pool->mutex);
  bool queued = !pool->stopping && pool->queue_count < POOL_QUEUE;
  if (queued) {
    pool->queue[(pool->queue_head + pool->queue_count) % POOL_QUEUE] = fd;
    pool->queue_count++;
    pthread_cond_broadcast(&pool->work);
  }
  pthread_mutex_unlock(&pool->mutex);
  if (!queued) {
    close(fd);
  }
}

static int take_connection(HttpPool *pool, int seat) {
  pthread_mutex_lock(&pool->mutex);
  while (!pool->stopping && pool->queue_count == 0) {
    (void)osp_parked_wait(&pool->work, &pool->mutex, NULL);
  }
  int fd = -1;
  if (!pool->stopping) {
    fd = pool->queue[pool->queue_head];
    pool->queue_head = (pool->queue_head + 1U) % POOL_QUEUE;
    pool->queue_count--;
    pool->serving[seat] = fd;
  }
  pthread_mutex_unlock(&pool->mutex);
  return fd;
}

static void hand_back(HttpPool *pool, int seat, int fd, bool keep) {
  pthread_mutex_lock(&pool->mutex);
  pool->serving[seat] = -1;
  bool kept = keep && !pool->stopping && pool->returned_count < POOL_QUEUE;
  if (kept) {
    pool->returned[pool->returned_count++] = fd;
  }
  pthread_mutex_unlock(&pool->mutex);
  if (kept) {
    wake_poller(pool);
  } else {
    close(fd);
  }
}

static int64_t worker_fiber(void *environment) {
  WorkerSeat *seat = environment;
  HttpPool *pool = seat->pool;
  pthread_mutex_lock(&pool->mutex);
  pool->threads[seat->index] = pthread_self();
  pool->thread_known[seat->index] = true;
  pthread_mutex_unlock(&pool->mutex);
  for (int fd; (fd = take_connection(pool, seat->index)) >= 0;) {
    hand_back(pool, seat->index, fd, http_serve_connection(pool->server, fd));
  }
  return 0;
}

static void stop_workers(HttpPool *pool) {
  pthread_mutex_lock(&pool->mutex);
  pool->stopping = true;
  pthread_cond_broadcast(&pool->work);
  pthread_mutex_unlock(&pool->mutex);
  for (int index = 0; index < pool->workers; index++) {
    (void)fiber_join(pool->fibers[index]);
    fiber_release(pool->fibers[index]);
    pool->fibers[index] = NULL;
  }
}

static bool init_locks(HttpPool *pool) {
  if (pthread_mutex_init(&pool->mutex, NULL) != 0) {
    return false;
  }
  if (pthread_cond_init(&pool->work, NULL) == 0) {
    return true;
  }
  pthread_mutex_destroy(&pool->mutex);
  return false;
}

static void destroy_pool(HttpPool *pool);

static HttpPool *create_pool(HttpServer *server) {
  HttpPool *pool = calloc(1, sizeof(*pool));
  if (!pool || !init_locks(pool)) {
    free(pool);
    return NULL;
  }
  if (!open_wake_pair(pool->wake)) {
    destroy_pool(pool);
    return NULL;
  }
  pool->server = server;
  for (int index = 0; index < POOL_WORKERS_MAX; index++) {
    pool->serving[index] = -1;
  }
  int wanted = worker_count();
  while (pool->workers < wanted) {
    WorkerSeat *seat = &pool->seats[pool->workers];
    seat->pool = pool;
    seat->index = pool->workers;
    Fiber *fiber = fiber_spawn_env(worker_fiber, seat);
    if (!fiber) {
      break;
    }
    pool->fibers[pool->workers++] = fiber;
  }
  return pool;
}

static void close_all(int *fds, size_t count) {
  for (size_t index = 0; index < count; index++) {
    close(fds[index]);
  }
}

static void destroy_pool(HttpPool *pool) {
  for (size_t index = 0; index < pool->idle_count; index++) {
    close(pool->idle[index].fd);
  }
  for (size_t index = 0; index < pool->queue_count; index++) {
    close(pool->queue[(pool->queue_head + index) % POOL_QUEUE]);
  }
  close_all(pool->returned, pool->returned_count);
  close_wake_pair(pool->wake);
  pthread_cond_destroy(&pool->work);
  pthread_mutex_destroy(&pool->mutex);
  free(pool);
}

static void drop_idle(HttpPool *pool, size_t index) {
  pool->idle[index] = pool->idle[--pool->idle_count];
}

// A readable idle connection holds its next request, or has been closed.
static bool peer_closed(int fd) {
  char byte;
  return recv(fd, &byte, 1, MSG_PEEK) <= 0;
}

static void adopt_returned(HttpPool *pool, uint64_t now) {
  pthread_mutex_lock(&pool->mutex);
  size_t count = pool->returned_count;
  int adopted[POOL_QUEUE];
  memcpy(adopted, pool->returned, count * sizeof(int));
  pool->returned_count = 0;
  pthread_mutex_unlock(&pool->mutex);
  for (size_t index = 0; index < count; index++) {
    if (pool->idle_count == POOL_IDLE_MAX) {
      close(adopted[index]);
    } else {
      pool->idle[pool->idle_count++] = (IdleConnection){adopted[index], now};
    }
  }
}

static void accept_client(HttpPool *pool) {
  int fd = accept(http_server_socket(pool->server), NULL, NULL);
  if (fd < 0) {
    return;
  }
  if (!http_configure_client_socket(fd) || !http_server_is_listening(pool->server)) {
    close(fd);
    return;
  }
  enqueue(pool, fd);
}

// Visit the idle connections polled at fds[2...]: queue those with a request,
// close those the client closed or left idle past the keep-alive timeout.
static void visit_idle(HttpPool *pool, struct pollfd *fds, size_t polled,
                       uint64_t now) {
  for (size_t slot = polled; slot > 0; slot--) {
    size_t index = slot - 1U;
    short events = fds[index + 2U].revents;
    int fd = pool->idle[index].fd;
    if (events != 0) {
      drop_idle(pool, index);
      if (peer_closed(fd)) {
        close(fd);
      } else {
        enqueue(pool, fd);
      }
    } else if (now - pool->idle[index].since_ms >= POOL_KEEP_ALIVE_MS) {
      drop_idle(pool, index);
      close(fd);
    }
  }
}

static void poll_once(HttpPool *pool, struct pollfd *fds) {
  pthread_mutex_lock(&pool->mutex);
  bool room = pool->queue_count < POOL_QUEUE;
  pthread_mutex_unlock(&pool->mutex);
  fds[0] = (struct pollfd){.fd = http_server_socket(pool->server),
                           .events = room ? POLLIN : 0};
  fds[1] = (struct pollfd){.fd = pool->wake[0], .events = POLLIN};
  size_t polled = pool->idle_count;
  for (size_t index = 0; index < polled; index++) {
    fds[index + 2U] = (struct pollfd){.fd = pool->idle[index].fd, .events = POLLIN};
  }
  jmp_buf registers;
  OSP_SPILL_REGISTERS(registers);
  osp_mem_baton_park(&registers);
  int ready = http_poll(fds, (unsigned)(polled + 2U), POOL_POLL_MS);
  osp_mem_baton_unpark();
  if (ready < 0) {
    return;
  }
  uint64_t now = osp_monotonic_ms();
  visit_idle(pool, fds, polled, now);
  if (fds[1].revents & POLLIN) {
    char drained[64];
    (void)recv(pool->wake[0], drained, sizeof(drained), 0);
    adopt_returned(pool, now);
  }
  if (fds[0].revents & POLLIN) {
    accept_client(pool);
  }
}

static void publish_pool(HttpServer *server, HttpPool *pool) {
  pthread_mutex_lock(&server->mutex);
  server->pool = pool;
  pthread_mutex_unlock(&server->mutex);
}

void http_pool_run(HttpServer *server) {
  HttpPool *pool = create_pool(server);
  struct pollfd *fds = pool && pool->workers > 0
                           ? calloc(POOL_IDLE_MAX + 2U, sizeof(*fds))
                           : NULL;
  if (!fds) {
    fprintf(stderr, "HTTP server on %s:%d could not start its connection workers\n",
            server->address, server->port);
  } else {
    publish_pool(server, pool);
    while (http_server_is_listening(server)) {
      poll_once(pool, fds);
    }
  }
  free(fds);
  if (pool) {
    stop_workers(pool);
    publish_pool(server, NULL);
    destroy_pool(pool);
  }
}

bool http_pool_owns_thread(HttpServer *server) {
  HttpPool *pool = server->pool;
  if (!pool) {
    return false;
  }
  pthread_mutex_lock(&pool->mutex);
  bool owned = false;
  for (int index = 0; index < pool->workers && !owned; index++) {
    owned = pool->thread_known[index] &&
            pthread_equal(pool->threads[index], pthread_self()) != 0;
  }
  pthread_mutex_unlock(&pool->mutex);
  return owned;
}

void http_pool_shutdown_clients(HttpServer *server) {
  HttpPool *pool = server->pool;
  if (!pool) {
    return;
  }
  pthread_mutex_lock(&pool->mutex);
  for (int index = 0; index < pool->workers; index++) {
    bool own = pool->thread_known[index] &&
               pthread_equal(pool->threads[index], pthread_self()) != 0;
    if (!own && pool->serving[index] >= 0) {
      (void)shutdown(pool->serving[index], HTTP_SHUTDOWN_BOTH);
    }
  }
  pthread_mutex_unlock(&pool->mutex);
}

size_t http_pool_serving(HttpServer *server) {
  pthread_mutex_lock(&server->mutex);
  HttpPool *pool = server->pool;
  size_t serving = 0;
  if (pool) {
    pthread_mutex_lock(&pool->mutex);
    for (int index = 0; index < pool->workers; index++) {
      serving += pool->serving[index] >= 0 ? 1U : 0U;
    }
    pthread_mutex_unlock(&pool->mutex);
  }
  pthread_mutex_unlock(&server->mutex);
  return serving;
}

void http_pool_enter_handler(HttpServer *server) {
  HttpPool *pool = server->pool;
  pthread_mutex_lock(&pool->mutex);
  while (pool->handling) {
    (void)osp_parked_wait(&pool->work, &pool->mutex, NULL);
  }
  pool->handling = true;
  pthread_mutex_unlock(&pool->mutex);
}

void http_pool_leave_handler(HttpServer *server) {
  HttpPool *pool = server->pool;
  pthread_mutex_lock(&pool->mutex);
  pool->handling = false;
  pthread_cond_broadcast(&pool->work);
  pthread_mutex_unlock(&pool->mutex);
}
