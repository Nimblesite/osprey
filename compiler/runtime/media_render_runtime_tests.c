// End-to-end tests for [FFI-MEDIA-RENDER]. Fake ffprobe/ffmpeg executables on
// PATH exercise the real spawn, playlist, process-stop and cleanup paths.
#include <assert.h>
#include <dirent.h>
#include <errno.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

char *osprey_media_render(char *audio);
int64_t osprey_media_unrender(char *folder);

static char ROOT[1024];
static char BIN[1024];
static char AUDIO[1024];

static const char *FFPROBE =
    "#!/bin/sh\n"
    "case ${FAKE_FFPROBE_MODE:-ok} in\n"
    "  ok) /bin/echo \"$FAKE_DURATION\" ;;\n"
    "  none) exit 0 ;;\n"
    "  invalid) /bin/echo not-a-duration ;;\n"
    "  failure) /bin/echo 12; exit 7 ;;\n"
    "  unwritable)\n"
    "    for folder in \"$TMPDIR\"/osprey-cast.*; do\n"
    "      [ -d \"$folder\" ] || continue\n"
    "      /bin/mkdir \"$folder/media.m3u8\"\n"
    "      /bin/chmod 0555 \"$folder\"\n"
    "    done\n"
    "    /bin/echo 8 ;;\n"
    "esac\n";

static const char *FFMPEG =
    "#!/bin/sh\n"
    "for last do :; done\n"
    "folder=${last%/*}\n"
    "printf '%s\\n' \"$@\" > \"$folder/args\"\n"
    "printf '%s\\n' $$ > \"$folder/renderer\"\n"
    "audio=; pattern=\n"
    "while [ \"$#\" -gt 0 ]; do\n"
    "  case \"$1\" in\n"
    "    -i) audio=$2; shift 2 ;;\n"
    "    -hls_segment_filename) pattern=$2; shift 2 ;;\n"
    "    *) shift ;;\n"
    "  esac\n"
    "done\n"
    "printf '%s' \"$audio\" > \"$folder/audio\"\n"
    "printf '#EXTM3U\\n' > \"$last\"\n"
    "first=$(printf \"$pattern\" 0)\n"
    "second=$(printf \"$pattern\" 1)\n"
    "printf zero > \"$first.tmp\"\n"
    "/bin/mv \"$first.tmp\" \"$first\"\n"
    "printf one > \"$second.tmp\"\n"
    "/bin/mv \"$second.tmp\" \"$second\"\n"
    ": > \"$folder/started\"\n"
    "trap 'exit 0' TERM INT\n"
    "while :; do /bin/sleep 1; done\n";

static void path(char *out, size_t size, const char *folder, const char *name) {
  assert(snprintf(out, size, "%s/%s", folder, name) > 0);
}

static void write_text(const char *file, const char *text) {
  FILE *stream = fopen(file, "w");
  assert(stream != NULL);
  assert(fputs(text, stream) >= 0);
  assert(fclose(stream) == 0);
}

static char *read_text(const char *file) {
  FILE *stream = fopen(file, "r");
  assert(stream != NULL);
  char *text = calloc(16384, 1);
  assert(text != NULL);
  assert(fread(text, 1, 16383, stream) < 16383);
  assert(!ferror(stream));
  assert(fclose(stream) == 0);
  return text;
}

static void assert_file(const char *folder, const char *name, const char *expected) {
  char file[1120];
  path(file, sizeof file, folder, name);
  char *actual = read_text(file);
  if (strcmp(actual, expected) != 0) {
    fprintf(stderr, "%s: expected <%s>, got <%s>\n", file, expected, actual);
  }
  assert(strcmp(actual, expected) == 0);
  free(actual);
}

static int exists(const char *file) {
  struct stat info;
  return stat(file, &info) == 0;
}

static void wait_for_start(const char *folder) {
  char file[1120];
  path(file, sizeof file, folder, "started");
  for (int i = 0; i < 3000 && !exists(file); i++) { // a loaded machine is slow to spawn
    usleep(10000);
  }
  assert(exists(file));
}

static long renderer_pid(const char *folder) {
  char file[1120];
  path(file, sizeof file, folder, "renderer");
  char *text = read_text(file);
  long pid = strtol(text, NULL, 10);
  free(text);
  assert(pid > 0);
  return pid;
}

static void install_tool(const char *name, const char *script) {
  char file[1120];
  path(file, sizeof file, BIN, name);
  write_text(file, script);
  assert(chmod(file, 0755) == 0);
}

static void remove_tool(const char *name) {
  char file[1120];
  path(file, sizeof file, BIN, name);
  assert(unlink(file) == 0);
}

static void assert_no_render_folders(void) {
  DIR *dir = opendir(ROOT);
  assert(dir != NULL);
  for (struct dirent *entry = readdir(dir); entry; entry = readdir(dir)) {
    assert(strncmp(entry->d_name, "osprey-cast.", 12) != 0);
  }
  assert(closedir(dir) == 0);
}

static void assert_render_fails(void) {
  char *folder = osprey_media_render(AUDIO);
  assert(folder != NULL && strcmp(folder, "") == 0);
  free(folder);
  assert_no_render_folders();
}

static void assert_arguments(const char *folder) {
  char file[1120];
  path(file, sizeof file, folder, "args");
  char *args = read_text(file);
  assert(strstr(args, "-filter_complex\n[0:a]showcqt=") != NULL);
  assert(strstr(args, "-hls_playlist_type\nvod\n") != NULL);
  assert(strstr(args, "-hls_flags\ntemp_file\n") != NULL);
  assert(strstr(args, "seg%05d.ts") != NULL);
  free(args);
}

static void assert_live_render(const char *folder, const char *playlist) {
  assert(strncmp(folder, ROOT, strlen(ROOT)) == 0);
  assert(strstr(folder, "/osprey-cast.") != NULL);
  assert_file(folder, "media.m3u8", playlist);
  wait_for_start(folder);
  assert_file(folder, "ffmpeg.m3u8", "#EXTM3U\n");
  assert_file(folder, "audio", AUDIO);
  assert_file(folder, "seg00000.ts", "zero");
  assert_file(folder, "seg00001.ts", "one");
  assert_arguments(folder);
  assert(kill(renderer_pid(folder), 0) == 0);
}

static void render_succeeds(const char *duration, const char *playlist) {
  assert(setenv("FAKE_DURATION", duration, 1) == 0);
  char *folder = osprey_media_render(AUDIO);
  assert(folder != NULL && folder[0] != '\0');
  assert_live_render(folder, playlist);
  long pid = renderer_pid(folder);
  assert(osprey_media_unrender(folder) == 0);
  assert(!exists(folder));
  errno = 0;
  assert(kill((pid_t)pid, 0) == -1 && errno == ESRCH);
  free(folder);
  assert_no_render_folders();
}

static void test_successful_playlists(void) {
  render_succeeds("10.25", "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:4\n"
                           "#EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PLAYLIST-TYPE:VOD\n"
                           "#EXTINF:4.000000,\nseg00000.ts\n#EXTINF:4.000000,\nseg00001.ts\n"
                           "#EXTINF:2.250000,\nseg00002.ts\n#EXT-X-ENDLIST\n");
  render_succeeds("8", "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:4\n"
                        "#EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PLAYLIST-TYPE:VOD\n"
                        "#EXTINF:4.000000,\nseg00000.ts\n#EXTINF:4.000000,\nseg00001.ts\n"
                        "#EXT-X-ENDLIST\n");
  render_succeeds("0.005", "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:4\n"
                           "#EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PLAYLIST-TYPE:VOD\n"
                           "#EXT-X-ENDLIST\n");
}

static void assert_blank_render(char *audio) {
  char *folder = osprey_media_render(audio);
  assert(folder != NULL && strcmp(folder, "") == 0);
  free(folder);
}

static void test_bad_inputs_and_temp_folder(void) {
  assert_blank_render(NULL);
  char empty[] = "";
  assert_blank_render(empty);
  char missing[] = "/definitely/not/an/audio/file";
  assert_blank_render(missing);
  assert_no_render_folders();
  char absent[1120];
  path(absent, sizeof absent, ROOT, "missing-tmp");
  assert(setenv("TMPDIR", absent, 1) == 0);
  assert_render_fails();
  assert(setenv("TMPDIR", ROOT, 1) == 0);
}

static void test_missing_tools(void) {
  remove_tool("ffprobe");
  assert_render_fails();
  install_tool("ffprobe", FFPROBE);
  remove_tool("ffmpeg");
  assert_render_fails();
  install_tool("ffmpeg", FFMPEG);
}

static void test_unreported_durations(void) {
  const char *modes[] = {"none", "invalid", "failure"};
  for (size_t i = 0; i < sizeof modes / sizeof modes[0]; i++) {
    assert(setenv("FAKE_FFPROBE_MODE", modes[i], 1) == 0);
    assert_render_fails();
  }
  assert(setenv("FAKE_FFPROBE_MODE", "ok", 1) == 0);
  assert(setenv("FAKE_DURATION", "0", 1) == 0);
  assert_render_fails();
  assert(setenv("FAKE_DURATION", "-2", 1) == 0);
  assert_render_fails();
}

static void find_render_folder(char *folder, size_t size) {
  DIR *dir = opendir(ROOT);
  assert(dir != NULL);
  struct dirent *entry;
  for (entry = readdir(dir); entry; entry = readdir(dir)) {
    if (strncmp(entry->d_name, "osprey-cast.", 12) == 0) {
      break;
    }
  }
  assert(entry != NULL);
  path(folder, size, ROOT, entry->d_name);
  assert(closedir(dir) == 0);
}

static void test_unwritable_playlist_folder(void) {
  assert(setenv("FAKE_FFPROBE_MODE", "unwritable", 1) == 0);
  char *result = osprey_media_render(AUDIO);
  assert(result != NULL && strcmp(result, "") == 0);
  free(result);
  char folder[1120];
  find_render_folder(folder, sizeof folder);
  char playlist[1120];
  path(playlist, sizeof playlist, folder, "media.m3u8");
  struct stat info;
  assert(stat(playlist, &info) == 0 && S_ISDIR(info.st_mode));
  assert(chmod(folder, 0700) == 0);
  assert(rmdir(playlist) == 0);
  assert(rmdir(folder) == 0);
  assert(setenv("FAKE_FFPROBE_MODE", "ok", 1) == 0);
}

static void make_folder(const char *suffix, char *folder, size_t size) {
  char name[128];
  assert(snprintf(name, sizeof name, "osprey-cast.%s", suffix) > 0);
  path(folder, size, ROOT, name);
  assert(mkdir(folder, 0700) == 0);
}

// A process that must outlive every refused unrender, and its folder's `pid` file.
static pid_t start_victim(void) {
  pid_t victim = fork();
  assert(victim >= 0);
  if (victim == 0) {
    execl("/bin/sleep", "sleep", "60", (char *)NULL);
    _exit(127);
  }
  return victim;
}

static void plant_pid(const char *folder, pid_t victim) {
  char file[1120];
  char text[32];
  path(file, sizeof file, folder, "pid");
  assert(snprintf(text, sizeof text, "%ld\n", (long)victim) > 0);
  write_text(file, text);
}

static void assert_alive_then_stop(pid_t victim) {
  assert(kill(victim, 0) == 0);
  int status = 0;
  assert(waitpid(victim, &status, WNOHANG) == 0); // still running, not just a zombie
  assert(kill(victim, SIGKILL) == 0 && waitpid(victim, &status, 0) == victim);
}

// Only a folder this process rendered counts, whatever its name says: a planted one
// must neither kill the process its `pid` file names nor lose a single file.
static void test_unrender_refuses_what_it_did_not_render(void) {
  assert(osprey_media_unrender(NULL) == -1);
  char unknown[1120];
  path(unknown, sizeof unknown, ROOT, "unknown");
  assert(osprey_media_unrender(unknown) == -1);
  char absent[1120];
  make_folder("absent", absent, sizeof absent);
  assert(rmdir(absent) == 0);
  assert(osprey_media_unrender(absent) == -1);
  pid_t victim = start_victim();
  char planted[1120];
  char keep[1120];
  char pid[1120];
  make_folder("planted", planted, sizeof planted);
  path(keep, sizeof keep, planted, "keep.txt");
  path(pid, sizeof pid, planted, "pid");
  write_text(keep, "precious");
  plant_pid(planted, victim);
  assert(osprey_media_unrender(planted) == -1);
  assert(exists(keep) && exists(pid));
  assert_alive_then_stop(victim);
  assert(unlink(keep) == 0 && unlink(pid) == 0 && rmdir(planted) == 0);
}

// A render's own folder is no more trusted than a planted one: only the child the
// runtime started is stopped, and only the exact folder it made is accepted, once.
static void test_unrender_trusts_only_its_own_record(void) {
  assert(setenv("FAKE_DURATION", "8", 1) == 0);
  char *folder = osprey_media_render(AUDIO);
  assert(folder != NULL && folder[0] != '\0');
  wait_for_start(folder);
  long renderer = renderer_pid(folder);
  pid_t victim = start_victim();
  plant_pid(folder, victim);
  char alias[1200];
  char *base = strrchr(folder, '/');
  assert(snprintf(alias, sizeof alias, "%s/.%s", ROOT, base) > 0);
  assert(osprey_media_unrender(alias) == -1);
  assert(exists(folder) && kill((pid_t)renderer, 0) == 0);
  assert(osprey_media_unrender(folder) == 0);
  assert(!exists(folder));
  errno = 0;
  assert(kill((pid_t)renderer, 0) == -1 && errno == ESRCH);
  assert_alive_then_stop(victim);
  assert(osprey_media_unrender(folder) == -1);
  free(folder);
  assert_no_render_folders();
}

// Two renders at once are told apart, and stopped in either order.
static void test_renders_are_independent(void) {
  assert(setenv("FAKE_DURATION", "8", 1) == 0);
  char *first = osprey_media_render(AUDIO);
  char *second = osprey_media_render(AUDIO);
  assert(first[0] != '\0' && second[0] != '\0' && strcmp(first, second) != 0);
  wait_for_start(first);
  wait_for_start(second);
  long second_renderer = renderer_pid(second);
  assert(osprey_media_unrender(first) == 0);
  assert(!exists(first) && exists(second) && kill((pid_t)second_renderer, 0) == 0);
  assert(osprey_media_unrender(second) == 0);
  assert(!exists(second));
  free(first);
  free(second);
  assert_no_render_folders();
}

// Past the record's capacity a render is refused, leaving no child or folder behind,
// and a stopped render makes room again.
static void test_renders_past_the_record(void) {
  enum { RECORDED = 16 };
  assert(setenv("FAKE_DURATION", "8", 1) == 0);
  char *folders[RECORDED];
  for (int i = 0; i < RECORDED; i++) {
    folders[i] = osprey_media_render(AUDIO);
    assert(folders[i][0] != '\0');
    wait_for_start(folders[i]);
  }
  char *refused = osprey_media_render(AUDIO);
  assert(refused[0] == '\0');
  free(refused);
  assert(osprey_media_unrender(folders[7]) == 0);
  free(folders[7]);
  folders[7] = osprey_media_render(AUDIO);
  assert(folders[7][0] != '\0');
  wait_for_start(folders[7]);
  for (int i = 0; i < RECORDED; i++) {
    assert(osprey_media_unrender(folders[i]) == 0);
    free(folders[i]);
  }
  assert_no_render_folders();
}

static void setup(void) {
  char folder[] = "/tmp/osprey-media-tests.XXXXXX";
  assert(mkdtemp(folder) != NULL);
  assert(snprintf(ROOT, sizeof ROOT, "%s", folder) > 0);
  path(BIN, sizeof BIN, ROOT, "bin");
  assert(mkdir(BIN, 0700) == 0);
  path(AUDIO, sizeof AUDIO, ROOT, "song.wav");
  write_text(AUDIO, "audio");
  assert(setenv("TMPDIR", ROOT, 1) == 0);
  assert(setenv("PATH", BIN, 1) == 0);
  assert(setenv("FAKE_FFPROBE_MODE", "ok", 1) == 0);
  install_tool("ffprobe", FFPROBE);
  install_tool("ffmpeg", FFMPEG);
}

static void cleanup(void) {
  char file[1120];
  path(file, sizeof file, BIN, "ffprobe");
  assert(unlink(file) == 0);
  path(file, sizeof file, BIN, "ffmpeg");
  assert(unlink(file) == 0);
  assert(rmdir(BIN) == 0);
  assert(unlink(AUDIO) == 0);
  assert(rmdir(ROOT) == 0);
}

int main(void) {
  setup();
  test_bad_inputs_and_temp_folder();
  test_missing_tools();
  test_unreported_durations();
  test_successful_playlists();
  test_unwritable_playlist_folder();
  test_unrender_refuses_what_it_did_not_render();
  test_unrender_trusts_only_its_own_record();
  test_renders_are_independent();
  test_renders_past_the_record();
  cleanup();
  printf("✅ media render runtime tests passed\n");
  return 0;
}
