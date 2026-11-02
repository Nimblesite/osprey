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
  for (int i = 0; i < 200 && !exists(file); i++) {
    usleep(10000);
  }
  assert(exists(file));
}

static long renderer_pid(const char *folder) {
  char file[1120];
  path(file, sizeof file, folder, "pid");
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

static void test_unrender_rejections_and_stale_pid(void) {
  assert(osprey_media_unrender(NULL) == -1);
  char unknown[1120];
  path(unknown, sizeof unknown, ROOT, "unknown");
  assert(osprey_media_unrender(unknown) == -1);
  char absent[1120];
  make_folder("absent", absent, sizeof absent);
  assert(rmdir(absent) == 0);
  assert(osprey_media_unrender(absent) == -1);
  char stale[1120];
  make_folder("stale", stale, sizeof stale);
  char pid[1120];
  path(pid, sizeof pid, stale, "pid");
  write_text(pid, "2147483647\n");
  assert(osprey_media_unrender(stale) == 0);
  assert(!exists(stale) && !exists(pid));
}

static void test_unrender_without_valid_pid(void) {
  char folder[1120];
  make_folder("no-pid", folder, sizeof folder);
  char media[1120];
  path(media, sizeof media, folder, "media.m3u8");
  write_text(media, "playlist");
  assert(osprey_media_unrender(folder) == 0);
  assert(!exists(folder) && !exists(media));
  make_folder("bad-pid", folder, sizeof folder);
  path(media, sizeof media, folder, "pid");
  write_text(media, "not-a-pid\n");
  assert(osprey_media_unrender(folder) == 0);
  assert(!exists(folder));
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
  test_unrender_rejections_and_stale_pid();
  test_unrender_without_valid_pid();
  cleanup();
  printf("✅ media render runtime tests passed\n");
  return 0;
}
