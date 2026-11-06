// term_runtime.c against a REAL terminal [BUILTIN-TERM]. Third translation unit
// of the builtins suite; see builtins_tests_shared.h.
//
// The behaviour half drives term_runtime with stdout a pipe, which proves the
// soft failures and the key decoder but never reaches what the functions exist
// for: raw mode entering and leaving, the window size of a terminal that has
// one, and the atexit handler that puts a wedged terminal back. Each scenario
// here runs in a forked child whose stdin and stdout are the slave side of a
// fresh pseudo-terminal; the child reads the master side to see the exact
// bytes written and asks the slave for its line discipline afterwards.
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/select.h>
#include <sys/wait.h>
#include <termios.h>
#include <unistd.h>

#include "builtins_tests_shared.h"
#include "test_gcov.h"

int64_t term_raw_mode(int64_t on);
int64_t term_cols(void);
int64_t term_rows(void);
char *term_read_key(void);

#define ENTER_RAW "\x1b[?1049h\x1b[2J\x1b[H"
#define LEAVE_RAW "\x1b[?1049l"
#define RESTORED "\x1b[?25h\x1b[?1049l"
enum { WAIT_MS = 2000, QUIET_MS = 100, PTY_COLS = 80, PTY_ROWS = 24 };

// Open a pseudo-terminal pair: answers the master, stores the slave.
static int open_pty(int *slave) {
  int master = posix_openpt(O_RDWR | O_NOCTTY);
  if (master < 0 || grantpt(master) != 0 || unlockpt(master) != 0) {
    return -1;
  }
  const char *name = ptsname(master);
  *slave = name == NULL ? -1 : open(name, O_RDWR | O_NOCTTY);
  return *slave < 0 ? -1 : master;
}

// Whether `fd` has bytes within `ms` milliseconds.
static int readable(int fd, int ms) {
  fd_set set;
  FD_ZERO(&set);
  FD_SET(fd, &set);
  struct timeval tv = {.tv_sec = ms / 1000, .tv_usec = (ms % 1000) * 1000};
  return select(fd + 1, &set, NULL, NULL, &tv) > 0;
}

// The master side receives EXACTLY `want`: nothing missing, nothing extra.
static int master_reads(int master, const char *want) {
  char got[256];
  size_t len = strlen(want);
  size_t have = 0;
  while (have < len && readable(master, WAIT_MS)) {
    ssize_t n = read(master, got + have, sizeof(got) - 1 - have);
    if (n <= 0) {
      return 0;
    }
    have += (size_t)n;
  }
  got[have] = '\0';
  return have == len && strcmp(got, want) == 0 && !readable(master, QUIET_MS);
}

// Whether every bit of `bits` is set in the slave's local modes.
static int local_modes(int fd, tcflag_t bits) {
  struct termios t;
  return tcgetattr(fd, &t) == 0 && (t.c_lflag & bits) == bits;
}

// Whether no bit of `bits` is set, and reads return after one byte.
static int raw_modes(int fd, tcflag_t bits) {
  struct termios t;
  return tcgetattr(fd, &t) == 0 && (t.c_lflag & bits) == 0 &&
         t.c_cc[VMIN] == 1 && t.c_cc[VTIME] == 0;
}

// Set the window the slave reports.
static int set_window(int slave, unsigned short cols, unsigned short rows) {
  struct winsize ws = {.ws_row = rows, .ws_col = cols};
  return ioctl(slave, TIOCSWINSZ, &ws) == 0;
}

// Raw mode leaves canonical input and echo, enters the alternate screen, and
// can be re-entered and left again; leaving twice writes nothing more.
static int raw_mode_round_trip(int master) {
  if (term_raw_mode(1) != 0 || !master_reads(master, ENTER_RAW)) {
    return 10;
  }
  if (!raw_modes(STDIN_FILENO, ICANON | ECHO)) {
    return 11;
  }
  if (term_raw_mode(1) != 0 || !master_reads(master, ENTER_RAW)) {
    return 12;
  }
  if (term_raw_mode(0) != 0 || !master_reads(master, LEAVE_RAW) ||
      !local_modes(STDIN_FILENO, ICANON | ECHO)) {
    return 13;
  }
  if (term_raw_mode(0) != 0 || !master_reads(master, "")) {
    return 14;
  }
  return 0;
}

// In raw mode one typed byte is a key at once -- no Enter -- and is not echoed.
static int raw_key_needs_no_enter(int master) {
  if (term_raw_mode(1) != 0 || !master_reads(master, ENTER_RAW)) {
    return 20;
  }
  if (write(master, "q", 1) != 1) {
    return 21;
  }
  char *key = term_read_key();
  int ok = key != NULL && strcmp(key, "q") == 0;
  free(key);
  return ok && master_reads(master, "") ? 0 : 22;
}

// A process that exits while raw restores the line discipline, shows the
// cursor and leaves the alternate screen without being asked.
static int exit_restores_the_terminal(int master) {
  fflush(stdout);
  pid_t pid = fork();
  if (pid < 0) {
    return 30;
  }
  if (pid == 0) {
    exit(0); // runs the atexit handler term_raw_mode registered
  }
  int status = 0;
  if (waitpid(pid, &status, 0) != pid || !WIFEXITED(status) ||
      WEXITSTATUS(status) != 0) {
    return 31;
  }
  if (!master_reads(master, RESTORED)) {
    return 32;
  }
  return local_modes(STDIN_FILENO, ICANON | ECHO) ? 0 : 33;
}

// Make a fresh pseudo-terminal this process's stdin and stdout; answers the
// master, stores the slave, or -1.
static int attach_pty(int *slave) {
  int master = open_pty(slave);
  if (master < 0 || !set_window(*slave, PTY_COLS, PTY_ROWS)) {
    return -1;
  }
  fflush(stdout);
  return dup2(*slave, STDIN_FILENO) < 0 || dup2(*slave, STDOUT_FILENO) < 0
             ? -1
             : master;
}

// Every scenario in order against one pseudo-terminal; 0 or the failing step.
static int tty_scenarios(void) {
  int slave = -1;
  int master = attach_pty(&slave);
  if (master < 0) {
    return 1;
  }
  if (term_cols() != PTY_COLS || term_rows() != PTY_ROWS) {
    return 3;
  }
  int step = raw_mode_round_trip(master);
  step = step != 0 ? step : raw_key_needs_no_enter(master);
  step = step != 0 ? step : exit_restores_the_terminal(master);
  if (step != 0) {
    return step;
  }
  // A window with no columns is "size unavailable", never a size of zero.
  return set_window(slave, 0, PTY_ROWS) && term_cols() == -1 &&
                 term_rows() == -1
             ? 0
             : 4;
}

void t_term_tty(void) {
  fflush(stdout);
  pid_t pid = fork();
  CHECK(pid >= 0);
  if (pid == 0) {
    int code = tty_scenarios();
    OSP_GCOV_DUMP();
    _exit(code);
  }
  int status = 0;
  CHECK(waitpid(pid, &status, 0) == pid);
  CHECK(WIFEXITED(status));
  if (WEXITSTATUS(status) != 0) {
    fprintf(stderr, "term tty scenario failed at step %d\n",
            WEXITSTATUS(status));
  }
  CHECK(WEXITSTATUS(status) == 0);
}
