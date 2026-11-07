// osprey_path.h — the ONE place the runtime builds a child path from a
// directory and an entry name. Every caller — listDir, the media render's
// folder sweep, the profiler's socket sweep and the file server's static
// route — asks this unit instead of spelling "%s/%s" again, so separator
// handling, trailing-slash collapse and truncation have a single definition.
#ifndef OSPREY_PATH_H
#define OSPREY_PATH_H

#include <stdio.h>
#include <string.h>

// The longest path the runtime forms from a directory and one entry name.
#define OSP_PATH_MAX 4096

// The one separator the runtime writes and collapses. Every supported host,
// Windows included, accepts '/', so a platform difference is a change here.
#define OSP_PATH_SEP '/'

// Writes `dir/name` into `out` (capacity `size`), collapsing any trailing
// separators of `dir` so "a//b" and "//x" can never form. Returns the written
// length excluding the terminator, or 0 (with `out` emptied) when `out` is NULL
// or too small, either component is NULL, or `name` is empty. Truncation is
// refused, never silent: a 0 return means the caller must not use the path.
// Defined here, inline, so every runtime unit and C test suite that includes
// this header gets the same definition without a link dependency.
static inline size_t osp_path_child(char *out, size_t size, const char *dir, const char *name) {
  if (out == NULL || size == 0 || dir == NULL || name == NULL || name[0] == '\0') {
    return 0;
  }
  size_t end = strlen(dir);
  while (end > 0 && dir[end - 1] == OSP_PATH_SEP) {
    end--;
  }
  int wrote = snprintf(out, size, "%.*s%c%s", (int)end, dir, OSP_PATH_SEP, name);
  if (wrote <= 0 || (size_t)wrote >= size) {
    out[0] = '\0';
    return 0;
  }
  return (size_t)wrote;
}

#endif // OSPREY_PATH_H
