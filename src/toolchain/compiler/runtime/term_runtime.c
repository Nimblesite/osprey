// Terminal control runtime - raw mode, key reading, and ANSI helpers.
// Implements [BUILTIN-TERM].
//
// POSIX builds use termios + ioctl. The original terminal state is saved on the
// first raw-mode enable and an atexit handler restores cooked mode and shows the
// cursor so an uncaught exit never leaves the terminal wedged. Windows builds
// expose the same symbols as graceful stubs.

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef _WIN32
#include <sys/ioctl.h>
#include <sys/select.h>
#include <termios.h>
#include <unistd.h>

static struct termios g_orig_termios;
static bool g_orig_saved = false;
static bool g_raw_active = false;

#define ANSI_SHOW_CURSOR "\x1b[?25h"
#define ANSI_HIDE_CURSOR "\x1b[?25l"
#define ANSI_CLEAR "\x1b[2J\x1b[H"
// Alternate screen buffer (what vim/htop/less use): a separate, no-scrollback
// surface. Entering it gives a clean canvas where clear+home redraws truly in
// place; leaving it restores the user's previous terminal contents verbatim.
#define ANSI_ALT_SCREEN_ON "\x1b[?1049h"
#define ANSI_ALT_SCREEN_OFF "\x1b[?1049l"

static bool g_alt_screen = false;

static void restore_terminal(void) {
  if (g_orig_saved && g_raw_active) {
    tcsetattr(STDIN_FILENO, TCSANOW, &g_orig_termios);
    g_raw_active = false;
  }
  fputs(ANSI_SHOW_CURSOR, stdout);
  if (g_alt_screen) {
    fputs(ANSI_ALT_SCREEN_OFF, stdout);
    g_alt_screen = false;
  }
  fflush(stdout);
}

// term_raw_mode(1) enables raw mode (no canonical line buffering, no echo) and
// switches to the alternate screen buffer so the TUI redraws in place without
// piling frames into scrollback; term_raw_mode(0) restores cooked mode and the
// original screen. ISIG stays enabled so Ctrl-C always terminates and the atexit
// handler can never leave the terminal stuck.
int64_t term_raw_mode(int64_t on) {
  if (on) {
    if (!g_orig_saved) {
      if (tcgetattr(STDIN_FILENO, &g_orig_termios) != 0) {
        return -1;
      }
      g_orig_saved = true;
      atexit(restore_terminal);
    }
    struct termios raw = g_orig_termios;
    raw.c_lflag &= ~((tcflag_t)(ICANON | ECHO));
    raw.c_cc[VMIN] = 1;
    raw.c_cc[VTIME] = 0;
    if (tcsetattr(STDIN_FILENO, TCSANOW, &raw) != 0) {
      return -2;
    }
    g_raw_active = true;
    fputs(ANSI_ALT_SCREEN_ON, stdout);
    fputs(ANSI_CLEAR, stdout);
    fflush(stdout);
    g_alt_screen = true;
    return 0;
  }
  if (g_orig_saved) {
    if (tcsetattr(STDIN_FILENO, TCSANOW, &g_orig_termios) != 0) {
      return -2;
    }
  }
  g_raw_active = false;
  if (g_alt_screen) {
    fputs(ANSI_ALT_SCREEN_OFF, stdout);
    fflush(stdout);
    g_alt_screen = false;
  }
  return 0;
}

static int64_t term_winsize(int which) {
  struct winsize ws;
  if (ioctl(STDOUT_FILENO, TIOCGWINSZ, &ws) != 0 || ws.ws_col == 0) {
    return -1;
  }
  return which == 0 ? (int64_t)ws.ws_col : (int64_t)ws.ws_row;
}

int64_t term_cols(void) { return term_winsize(0); }
int64_t term_rows(void) { return term_winsize(1); }

// Reads a single byte; returns -1 on EOF/error.
static int read_one_byte(void) {
  unsigned char ch;
  ssize_t n = read(STDIN_FILENO, &ch, 1);
  if (n != 1) {
    return -1;
  }
  return (int)ch;
}

// Reads one more byte if it arrives within timeout_ms; -1 if none.
static int read_byte_timeout(int timeout_ms) {
  fd_set set;
  FD_ZERO(&set);
  FD_SET(STDIN_FILENO, &set);
  struct timeval tv;
  tv.tv_sec = timeout_ms / 1000;
  tv.tv_usec = (timeout_ms % 1000) * 1000;
  int rv = select(STDIN_FILENO + 1, &set, NULL, NULL, &tv);
  if (rv <= 0) {
    return -1;
  }
  return read_one_byte();
}

#define ESC_TIMEOUT_MS 50

// term_read_key reads one keystroke and returns a human-readable name as a
// freshly allocated string ("Up"/"Enter"/"Esc"/"Ctrl-C"/literal char), or NULL
// on EOF/error.
char *term_read_key(void) {
  int ch = read_one_byte();
  if (ch < 0) {
    return NULL;
  }

  switch (ch) {
  case '\r':
  case '\n':
    return strdup("Enter");
  case 0x7f:
  case 0x08:
    return strdup("Backspace");
  case '\t':
    return strdup("Tab");
  case 0x03:
    return strdup("Ctrl-C");
  default:
    break;
  }

  if (ch == 0x1b) {
    int b1 = read_byte_timeout(ESC_TIMEOUT_MS);
    if (b1 < 0) {
      return strdup("Esc");
    }
    if (b1 == '[' || b1 == 'O') {
      int b2 = read_byte_timeout(ESC_TIMEOUT_MS);
      switch (b2) {
      case 'A': return strdup("Up");
      case 'B': return strdup("Down");
      case 'C': return strdup("Right");
      case 'D': return strdup("Left");
      case 'H': return strdup("Home");
      case 'F': return strdup("End");
      default:
        break;
      }
      if (b2 >= '0' && b2 <= '9') {
        int b3 = read_byte_timeout(ESC_TIMEOUT_MS); // consume trailing '~'
        (void)b3;
        switch (b2) {
        case '3': return strdup("Delete");
        case '5': return strdup("PageUp");
        case '6': return strdup("PageDown");
        default:
          break;
        }
      }
    }
    return strdup("Esc");
  }

  char buf[2] = {(char)ch, '\0'};
  return strdup(buf);
}

static int64_t term_write(const char *s) {
  fputs(s, stdout);
  fflush(stdout);
  return 0;
}

int64_t term_clear(void) { return term_write(ANSI_CLEAR); }
int64_t term_hide_cursor(void) { return term_write(ANSI_HIDE_CURSOR); }
int64_t term_show_cursor(void) { return term_write(ANSI_SHOW_CURSOR); }

int64_t term_move_cursor(int64_t row, int64_t col) {
  if (row < 1) {
    row = 1;
  }
  if (col < 1) {
    col = 1;
  }
  printf("\x1b[%lld;%lldH", (long long)row, (long long)col);
  fflush(stdout);
  return 0;
}

#else // _WIN32 - graceful stubs so the symbols exist everywhere.

int64_t term_raw_mode(int64_t on) {
  (void)on;
  return -1;
}
int64_t term_cols(void) { return -1; }
int64_t term_rows(void) { return -1; }
char *term_read_key(void) { return NULL; }
int64_t term_clear(void) { return -1; }
int64_t term_hide_cursor(void) { return -1; }
int64_t term_show_cursor(void) { return -1; }
int64_t term_move_cursor(int64_t row, int64_t col) {
  (void)row;
  (void)col;
  return -1;
}

#endif
