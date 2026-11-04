// Exercise the actual symbolic parser against the host libc oracle.
#include "hunt_mock_tests.h"
#include <errno.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
int64_t osp_parse_int_strict(const char *text, int64_t *out);

static char *queued_line(const char *bytes, size_t length) {
  hunt_reset();
  osp_hunt_input_bytes = (long)length;
  hunt_queue_bytes(bytes, length);
  hunt_queue_range((int)length);
  char *line = osp_input();
  hunt_set_object_size(length + 1);
  return line;
}

static void check_parse(const char *bytes, size_t length) {
  char *line = queued_line(bytes, length);
  hunt_check(line != NULL, "parser input allocation succeeds");
  if (!line) return;
  int64_t value = 173;
  int status = (int)osp_parse_int_strict(line, &value);
  char *end = NULL;
  errno = 0;
  long long expected = strtoll(line, &end, 10);
  int first = line[0];
  int start = (first >= '0' && first <= '9') || first == '+' || first == '-';
  int valid = start && errno != ERANGE && end != line && *end == '\0';
  hunt_check((status == 0) == valid && value == (valid ? expected : 173), "bounded parser matches host value and preserves failure output");
  free(line);
}

static void every_byte_matches_the_host_parser(void) {
  const char *seeds[] = {"0", "1234567890123456789", "9223372036854775807", "-9223372036854775808", "+00000000000000000000000000000000000000000000123"};
  for (size_t row = 0; row < sizeof(seeds) / sizeof(seeds[0]); row++) {
    size_t length = strlen(seeds[row]);
    char bytes[65];
    for (size_t at = 0; at < length; at++)
      for (unsigned byte = 0; byte < 256; byte++) {
        memcpy(bytes, seeds[row], length);
        bytes[at] = (char)byte;
        check_parse(bytes, length);
      }
  }
}

static void capacity_is_limited_to_the_live_exact_input_object(void) {
  char *line = queued_line("12", 2);
  hunt_check(osp_hunt_input_capacity(line) == 3, "input capacity includes its terminator");
  hunt_set_object_size(1);
  hunt_check(osp_hunt_input_capacity(line) == 1, "physical shrink cannot leave a stale larger capacity");
  hunt_set_object_size(100);
  hunt_check(osp_hunt_input_capacity(line) == 3, "a larger physical object does not enlarge the recorded input");
  hunt_check(osp_hunt_input_capacity(line + 1) == 0 && osp_hunt_input_capacity("12") == 0, "interior and foreign strings take the generic parser");
  hunt_reset();
  hunt_check(osp_hunt_input_capacity(line) == 0, "reset discards the input hint");
  free(line);
}

void hunt_mock_parse_tests(void) {
  capacity_is_limited_to_the_live_exact_input_object();
  every_byte_matches_the_host_parser();
  check_parse("", 0);
  check_parse("-9223372036854775809", 20);
  check_parse("9223372036854775808", 19);
  check_parse("18446744073709551616", 20);
  char zeros[64];
  memset(zeros, '0', sizeof(zeros));
  check_parse(zeros, sizeof(zeros));
}
