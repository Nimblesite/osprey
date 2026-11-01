// An audio file rendered as a visualiser video for a TV, as HLS
// [FFI-MEDIA-RENDER].
//
// A Cast receiver shows only still artwork for audio, so a sender that wants
// the TV to move with the music sends video. `osprey_media_render` makes a
// fresh folder and writes `media.m3u8` at once: a complete VOD playlist of
// 4-second segments computed from the duration `ffprobe` reports, so the
// receiver knows the whole length and can seek from the first moment. It
// then starts `ffmpeg` in the background drawing a constant-Q spectrum of the
// audio and cutting `segNNNNN.ts` files on exactly those 4-second keyframes.
// ffmpeg writes each segment under a temporary name and renames it when
// complete, and the file server's folder mode waits for a segment that is
// not there yet ([FFI-FILE-SERVER-FOLDER]). Rendering runs far faster than
// playback, so the wait happens only after a seek far ahead.
//
// `osprey_media_unrender` stops ffmpeg and deletes the folder. Without
// ffmpeg and ffprobe on the PATH, render returns "" and a sender casts the
// audio as it is. Windows has no posix_spawn, so there render always takes
// that path.

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#ifdef _WIN32

char *osprey_media_render(char *audio) {
  (void)audio;
  return strdup("");
}

int64_t osprey_media_unrender(char *folder) {
  (void)folder;
  return -1;
}

#else

#include <dirent.h>
#include <fcntl.h>
#include <signal.h>
#include <spawn.h>
#include <stdbool.h>
#include <stdio.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

// glibc declares environ under _GNU_SOURCE; Apple's headers never do.
#ifdef __APPLE__
extern char **environ;
#endif

enum { MR_SEGMENT = 4, MR_PATH = 1024 };

static const char *MR_FILTER =
    "[0:a]showcqt=s=1280x720:fps=30:axis=0:bar_g=2:sono_g=3:count=2,format=yuv420p[v]";

int64_t osprey_media_unrender(char *folder);

static char *mr_copy(const char *text) { return strdup(text ? text : ""); }

// `folder/name` into `path`; false when it does not fit, so a long folder can
// never make a truncated path that names some other file.
static bool mr_path(char path[MR_PATH], const char *folder, const char *name) {
  int length = snprintf(path, MR_PATH, "%s/%s", folder, name);
  return length >= 0 && length < MR_PATH;
}

// Spawns `args` (NULL-terminated) from the PATH with `actions`; its pid, or 0.
// posix_spawnp takes mutable strings, so the arguments are copied first.
static pid_t spawn(const char *const args[], const posix_spawn_file_actions_t *actions) {
  size_t count = 0;
  while (args[count]) {
    count++;
  }
  char **argv = calloc(count + 1, sizeof *argv);
  bool copied = argv != NULL;
  for (size_t i = 0; copied && i < count; i++) {
    argv[i] = strdup(args[i]);
    copied = argv[i] != NULL;
  }
  pid_t pid = 0;
  if (copied && posix_spawnp(&pid, argv[0], actions, NULL, argv, environ) != 0) {
    pid = 0;
  }
  for (size_t i = 0; argv && i < count; i++) {
    free(argv[i]);
  }
  free(argv);
  return pid;
}

// The duration ffprobe reports for `path`, in seconds; 0 when it cannot.
static double probe_duration(const char *path) {
  int pipefd[2];
  if (pipe(pipefd) != 0) {
    return 0;
  }
  posix_spawn_file_actions_t actions;
  posix_spawn_file_actions_init(&actions);
  posix_spawn_file_actions_adddup2(&actions, pipefd[1], STDOUT_FILENO);
  posix_spawn_file_actions_addclose(&actions, pipefd[0]);
  posix_spawn_file_actions_addopen(&actions, STDERR_FILENO, "/dev/null", O_WRONLY, 0);
  const char *const args[] = {"ffprobe", "-v", "error", "-show_entries", "format=duration",
                              "-of", "default=nw=1:nk=1", path, NULL};
  pid_t pid = spawn(args, &actions);
  int spawned = pid > 0 ? 0 : -1;
  posix_spawn_file_actions_destroy(&actions);
  close(pipefd[1]);
  char text[64] = "";
  ssize_t got = spawned == 0 ? read(pipefd[0], text, sizeof text - 1) : 0;
  close(pipefd[0]);
  text[got > 0 ? got : 0] = '\0';
  int status = 0;
  if (spawned == 0) {
    waitpid(pid, &status, 0);
  }
  double seconds = strtod(text, NULL);
  return spawned == 0 && WIFEXITED(status) && WEXITSTATUS(status) == 0 && seconds > 0 ? seconds
                                                                                         : 0;
}

// The VOD playlist for `seconds` of media in MR_SEGMENT-second segments.
static bool write_playlist(const char *folder, double seconds) {
  char path[MR_PATH];
  FILE *file = mr_path(path, folder, "media.m3u8") ? fopen(path, "w") : NULL;
  if (!file) {
    return false;
  }
  fprintf(file, "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:%d\n"
                "#EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PLAYLIST-TYPE:VOD\n", MR_SEGMENT);
  long whole = (long)(seconds / MR_SEGMENT);
  for (long i = 0; i < whole; i++) {
    fprintf(file, "#EXTINF:%d.000000,\nseg%05ld.ts\n", MR_SEGMENT, i);
  }
  double rest = seconds - (double)whole * MR_SEGMENT;
  if (rest > 0.01) {
    fprintf(file, "#EXTINF:%f,\nseg%05ld.ts\n", rest, whole);
  }
  fputs("#EXT-X-ENDLIST\n", file);
  return fclose(file) == 0;
}

// Starts ffmpeg rendering `audio` into `folder`; its pid, or 0.
static pid_t start_ffmpeg(const char *audio, const char *folder) {
  char segments[MR_PATH];
  char own[MR_PATH];
  if (!mr_path(segments, folder, "seg%05d.ts") || !mr_path(own, folder, "ffmpeg.m3u8")) {
    return 0;
  }
  const char *const args[] = {"ffmpeg", "-nostdin", "-loglevel", "error", "-i", audio,
                  "-filter_complex", MR_FILTER, "-map", "[v]", "-map", "0:a",
                  "-c:v", "libx264", "-preset", "veryfast",
                  "-force_key_frames", "expr:gte(t,n_forced*4)",
                  "-c:a", "aac", "-b:a", "192k", "-ac", "2",
                  "-f", "hls", "-hls_time", "4", "-hls_playlist_type", "vod",
                  "-hls_flags", "temp_file", "-hls_segment_filename", segments, own, NULL};
  posix_spawn_file_actions_t actions;
  posix_spawn_file_actions_init(&actions);
  posix_spawn_file_actions_addopen(&actions, STDOUT_FILENO, "/dev/null", O_WRONLY, 0);
  pid_t pid = spawn(args, &actions);
  posix_spawn_file_actions_destroy(&actions);
  return pid;
}

static void write_pid(const char *folder, pid_t pid) {
  char path[MR_PATH];
  FILE *file = mr_path(path, folder, "pid") ? fopen(path, "w") : NULL;
  if (file) {
    fprintf(file, "%ld\n", (long)pid);
    fclose(file);
  }
}

// osprey_media_render renders `audio` as a visualiser HLS stream and returns
// the folder holding `media.m3u8`, or "" when it cannot.
char *osprey_media_render(char *audio) {
  const char *tmp = getenv("TMPDIR");
  char folder[MR_PATH];
  struct stat info;
  if (!audio || stat(audio, &info) != 0 ||
      !mr_path(folder, tmp && *tmp ? tmp : "/tmp", "osprey-cast.XXXXXX") || !mkdtemp(folder)) {
    return mr_copy("");
  }
  double seconds = probe_duration(audio);
  pid_t pid = seconds > 0 && write_playlist(folder, seconds) ? start_ffmpeg(audio, folder) : 0;
  if (pid == 0) {
    char *copy = mr_copy(folder);
    osprey_media_unrender(copy);
    free(copy);
    return mr_copy("");
  }
  write_pid(folder, pid);
  return mr_copy(folder);
}

static void stop_renderer(const char *folder) {
  char path[MR_PATH];
  FILE *file = mr_path(path, folder, "pid") ? fopen(path, "r") : NULL;
  long pid = 0;
  if (file) {
    if (fscanf(file, "%ld", &pid) != 1) {
      pid = 0;
    }
    fclose(file);
  }
  if (pid > 0) {
    kill((pid_t)pid, SIGTERM);
    waitpid((pid_t)pid, NULL, 0);
  }
}

// osprey_media_unrender stops the render in `folder` and deletes the folder:
// 0, or -1 when `folder` is not one render made.
int64_t osprey_media_unrender(char *folder) {
  if (!folder || !strstr(folder, "/osprey-cast.")) {
    return -1;
  }
  stop_renderer(folder);
  DIR *dir = opendir(folder);
  if (!dir) {
    return -1;
  }
  char path[MR_PATH];
  for (struct dirent *entry = readdir(dir); entry; entry = readdir(dir)) {
    if (entry->d_name[0] != '.' && mr_path(path, folder, entry->d_name)) {
      unlink(path);
    }
  }
  closedir(dir);
  return rmdir(folder) == 0 ? 0 : -1;
}

#endif
