// The file and folder helpers the file server suites share, written once for POSIX
// and Windows so the same assertions run on both ([FFI-FILE-SERVER-FOLDER]: the
// hostile-name cases matter most on Windows, where a backslash or colon climbs out).
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <sys/stat.h>
#ifdef _WIN32
#include <direct.h>
#include <io.h>
#include <process.h>
#endif

// Last: on Windows it maps close() to closesocket(), which would rename the CRT's
// own close() in any system header after it.
#include "file_server_tests.h"

// The system's temporary folder: TEMP on Windows, /tmp elsewhere.
static const char *temp_root(void) {
#ifdef _WIN32
  const char *root = getenv("TEMP");
  return root != NULL ? root : ".";
#else
  return "/tmp";
#endif
}

#ifdef _WIN32
// `<temp>/<stem>.<pid>.<n>` for the n-th attempt at a fresh name.
static void numbered(char *out, size_t cap, const char *stem, unsigned n) {
  snprintf(out, cap, "%s/%s.%d.%u", temp_root(), stem, _getpid(), n);
}
#endif

int fs_temp_file(char *out, size_t cap, const char *stem) {
#ifdef _WIN32
  int fd = -1;
  for (unsigned n = 0; fd < 0; n++) {
    numbered(out, cap, stem, n);
    fd = _open(out, _O_CREAT | _O_EXCL | _O_RDWR | _O_BINARY, _S_IREAD | _S_IWRITE);
    assert(fd >= 0 || errno == EEXIST);
  }
#else
  snprintf(out, cap, "%s/%s.XXXXXX", temp_root(), stem);
  int fd = mkstemp(out);
  assert(fd >= 0);
#endif
  return fd;
}

void fs_temp_folder(char *out, size_t cap, const char *stem) {
#ifdef _WIN32
  for (unsigned n = 0;; n++) {
    numbered(out, cap, stem, n);
    if (_mkdir(out) == 0) {
      return;
    }
    assert(errno == EEXIST);
  }
#else
  snprintf(out, cap, "%s/%s.XXXXXX", temp_root(), stem);
  assert(mkdtemp(out) != NULL);
#endif
}

void fs_close_file(int fd) {
#ifdef _WIN32
  assert(_close(fd) == 0);
#else
  assert(close(fd) == 0);
#endif
}

void fs_make_folder(const char *path) {
#ifdef _WIN32
  assert(_mkdir(path) == 0);
#else
  assert(mkdir(path, 0700) == 0);
#endif
}

bool fs_peer_reset(void) {
#ifdef _WIN32
  return osp_socket_error() == WSAECONNRESET;
#else
  return errno == ECONNRESET;
#endif
}
