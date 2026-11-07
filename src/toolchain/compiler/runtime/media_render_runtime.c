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
// `osprey_media_unrender` stops ffmpeg and deletes the folder, for a folder this
// process rendered: the runtime's own record says which those are and which child
// to stop, and nothing on disk is trusted. An unreaped child keeps its pid, so that
// pid can't have come to mean another process. Without
// ffmpeg and ffprobe on the PATH, render returns "" and a sender casts the
// audio as it is.
//
// On Windows the casting app runs in an AppContainer ([WINUI-HOST-SANDBOX]),
// which cannot start ffmpeg, so render always returns "" there.

#ifdef __APPLE__
#include <TargetConditionals.h>
#endif

// Sandboxed mobile apps cannot launch the desktop ffmpeg process. The media
// contract explicitly falls back to the original audio when rendering is absent.
#if defined(_WIN32) || defined(__ANDROID__) || (defined(__APPLE__) && TARGET_OS_IPHONE)
#include <stdint.h>
#include <string.h>

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
#include <pthread.h>
#include <signal.h>
#include <spawn.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>
#include "osprey_path.h"

#if !(defined(__GLIBC__) && defined(_GNU_SOURCE))
extern char **environ;
#endif

// MR_FILE holds the folder (under MR_PATH) and a file name inside it.
enum { MR_SEGMENT = 4, MR_PATH = 1024, MR_FILE = MR_PATH + 32, MR_RENDERS = 16 };

static const char *MR_FILTER =
    "[0:a]showcqt=s=1280x720:fps=30:axis=0:bar_g=2:sono_g=3:count=2,format=yuv420p[v]";

// A render still running or not yet stopped: the folder and child that make it ours.
typedef struct {
  char folder[MR_PATH];
  pid_t pid; // 0: a free slot
} Render;

static Render mr_renders[MR_RENDERS];
static pthread_mutex_t mr_mutex = PTHREAD_MUTEX_INITIALIZER;

static char *mr_copy(const char *text) { return strdup(text ? text : ""); }

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
  char path[MR_FILE];
  snprintf(path, sizeof path, "%s/media.m3u8", folder);
  FILE *file = fopen(path, "w");
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
  char segments[MR_FILE];
  char own[MR_FILE];
  snprintf(segments, sizeof segments, "%s/seg%%05d.ts", folder);
  snprintf(own, sizeof own, "%s/ffmpeg.m3u8", folder);
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

// False when as many renders as the record holds are running.
static bool remember_render(const char *folder, pid_t pid) {
  bool remembered = false;
  pthread_mutex_lock(&mr_mutex);
  for (int i = 0; i < MR_RENDERS && !remembered; i++) {
    if (mr_renders[i].pid == 0) {
      snprintf(mr_renders[i].folder, sizeof mr_renders[i].folder, "%s", folder);
      mr_renders[i].pid = pid;
      remembered = true;
    }
  }
  pthread_mutex_unlock(&mr_mutex);
  return remembered;
}

// Removes the render of exactly this folder from the record: its child, or 0 when
// no render made the folder or it has already been stopped.
static pid_t forget_render(const char *folder) {
  pid_t pid = 0;
  pthread_mutex_lock(&mr_mutex);
  for (int i = 0; i < MR_RENDERS && pid == 0; i++) {
    if (mr_renders[i].pid != 0 && strcmp(mr_renders[i].folder, folder) == 0) {
      pid = mr_renders[i].pid;
      mr_renders[i].pid = 0;
    }
  }
  pthread_mutex_unlock(&mr_mutex);
  return pid;
}

static void stop_renderer(pid_t pid) {
  kill(pid, SIGTERM);
  waitpid(pid, NULL, 0);
}

// Deletes a render's folder and what is in it: 0, or -1 when it can't.
static int64_t delete_folder(const char *folder) {
  DIR *dir = opendir(folder);
  if (!dir) {
    return -1;
  }
  char path[MR_FILE];
  for (struct dirent *entry = readdir(dir); entry; entry = readdir(dir)) {
    if (entry->d_name[0] != '.' && osp_path_child(path, sizeof path, folder, entry->d_name) != 0) {
      unlink(path);
    }
  }
  closedir(dir);
  return rmdir(folder) == 0 ? 0 : -1;
}

// osprey_media_render renders `audio` as a visualiser HLS stream and returns
// the folder holding `media.m3u8`, or "" when it cannot.
char *osprey_media_render(char *audio) {
  const char *tmp = getenv("TMPDIR");
  char folder[MR_PATH];
  snprintf(folder, sizeof folder, "%s/osprey-cast.XXXXXX", tmp && *tmp ? tmp : "/tmp");
  struct stat info;
  if (!audio || stat(audio, &info) != 0 || !mkdtemp(folder)) {
    return mr_copy("");
  }
  double seconds = probe_duration(audio);
  pid_t pid = seconds > 0 && write_playlist(folder, seconds) ? start_ffmpeg(audio, folder) : 0;
  if (pid != 0 && !remember_render(folder, pid)) {
    stop_renderer(pid);
    pid = 0;
  }
  if (pid == 0) {
    (void)delete_folder(folder);
    return mr_copy("");
  }
  return mr_copy(folder);
}

// osprey_media_unrender stops the render in `folder` and deletes the folder:
// 0, or -1 when `folder` is not one this process's render made.
int64_t osprey_media_unrender(char *folder) {
  pid_t pid = folder ? forget_render(folder) : 0;
  if (pid == 0) {
    return -1;
  }
  stop_renderer(pid);
  return delete_folder(folder);
}
#endif
