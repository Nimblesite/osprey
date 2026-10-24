// Mock <pthread.h> for symbolic execution [SYMEX-MOCK-HEADERS]. KLEE explores
// one thread: mutexes are counters, thread creation is unmodelled
// [SYMEX-MOCK-UNSUPPORTED].
#ifndef OSP_HUNT_PTHREAD_H
#define OSP_HUNT_PTHREAD_H
typedef unsigned long pthread_t;
typedef struct { int locked; } pthread_mutex_t;
typedef struct { int unused; } pthread_mutexattr_t;
typedef struct { int unused; } pthread_attr_t;
#define PTHREAD_MUTEX_INITIALIZER { 0 }
int pthread_mutex_init(pthread_mutex_t *mutex, const pthread_mutexattr_t *attributes);
int pthread_mutex_destroy(pthread_mutex_t *mutex);
int pthread_mutex_lock(pthread_mutex_t *mutex);
int pthread_mutex_unlock(pthread_mutex_t *mutex);
int pthread_create(pthread_t *thread, const pthread_attr_t *attributes,
                   void *(*start)(void *), void *argument);
int pthread_join(pthread_t thread, void **result);
int pthread_detach(pthread_t thread);
pthread_t pthread_self(void);
#endif
