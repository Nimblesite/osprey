// osprey_path_open.h — opens a directory entry for reading without following a
// link in its last component, so a link swapped in after any earlier look at the
// name is refused, not followed ([FFI-FILE-SERVER-FOLDER]). Callers then check and
// read through the one handle this returns, never through the name again.
#ifndef OSPREY_PATH_OPEN_H
#define OSPREY_PATH_OPEN_H

#include <errno.h>
#include <fcntl.h>
#include <stdbool.h>
#include <stdio.h>

#ifdef _WIN32
#include <io.h>
#include <windows.h>

// The descriptor, or -1; a reparse point (symlink or junction) is refused.
static inline int osp_open_entry_fd(const char *path, bool *missing) {
  HANDLE handle = CreateFileA(path, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                              NULL, OPEN_EXISTING, FILE_FLAG_OPEN_REPARSE_POINT, NULL);
  if (handle == INVALID_HANDLE_VALUE) {
    DWORD error = GetLastError();
    // A writer holding the entry without read sharing has not finished it: wait, as for absent.
    *missing = error == ERROR_FILE_NOT_FOUND || error == ERROR_PATH_NOT_FOUND ||
               error == ERROR_SHARING_VIOLATION;
    return -1;
  }
  BY_HANDLE_FILE_INFORMATION info;
  bool plain = GetFileInformationByHandle(handle, &info) &&
               (info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT) == 0;
  int fd = plain ? _open_osfhandle((intptr_t)handle, _O_RDONLY | _O_BINARY) : -1;
  if (fd < 0) {
    CloseHandle(handle);
  }
  return fd;
}

#define osp_close_entry_fd _close
#else
#include <unistd.h>

// The descriptor, or -1. O_NONBLOCK: opening a planted FIFO must not block the caller.
static inline int osp_open_entry_fd(const char *path, bool *missing) {
  int fd = open(path, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC);
  *missing = fd < 0 && errno == ENOENT;
  return fd;
}

#define osp_close_entry_fd close
#endif

// The entry as a binary stream, or NULL with `*missing` set when it is not there, or
// not finished, yet. The descriptor never leaks.
static inline FILE *osp_open_entry(const char *path, bool *missing) {
  *missing = false;
  int fd = osp_open_entry_fd(path, missing);
  FILE *file = fd >= 0 ? fdopen(fd, "rb") : NULL;
  if (fd >= 0 && !file) {
    osp_close_entry_fd(fd);
  }
  return file;
}

#endif
