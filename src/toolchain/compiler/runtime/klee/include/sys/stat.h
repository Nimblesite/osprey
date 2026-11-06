// Mock <sys/stat.h> for symbolic execution [SYMEX-MOCK-HEADERS]; `stat` is
// unmodelled [SYMEX-MOCK-UNSUPPORTED].
#ifndef OSP_HUNT_SYS_STAT_H
#define OSP_HUNT_SYS_STAT_H
#include <sys/types.h>
#define S_ISREG(mode) (((mode) & 0170000) == 0100000)
struct stat {
  off_t st_size;
  mode_t st_mode;
};
int stat(const char *path, struct stat *out);
int mkdir(const char *path, mode_t mode);
#endif
