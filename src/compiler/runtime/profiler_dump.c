// Osprey sampling CPU profiler — JSON writers shared by the raw profile file
// and the live-attach stream. Implements [PROF-RAW-FORMAT] and the record
// shapes of [PROF-ATTACH-PROTOCOL] (docs/specs/tooling/0028-Profiler.md).
#include "profiler_runtime.h"

#if !defined(_WIN32) && !defined(__wasm__)

#include <inttypes.h>
#include <string.h>
#include <unistd.h>

#if defined(__APPLE__)
#include <mach-o/dyld.h>
#include <mach-o/dyld_images.h>
#include <mach-o/getsect.h>
#include <mach-o/loader.h>
#include <mach/mach.h>
#include <mach/task_info.h>
#else
#include <link.h>
#endif

void osp_prof_json_string(FILE *f, const char *s) {
  fputc('"', f);
  for (; *s; s++) {
    unsigned char c = (unsigned char)*s;
    if (c == '"' || c == '\\') {
      fputc('\\', f);
      fputc(c, f);
    } else if (c < 0x20) {
      fprintf(f, "\\u%04x", (unsigned)c);
    } else {
      fputc(c, f);
    }
  }
  fputc('"', f);
}

#if defined(__APPLE__)
// [PROF-SYMBOLIZE-OFFLINE] The mach header address alone cannot decide which
// image owns a pc. In the dyld shared cache every system dylib's header lives in
// one region and its __TEXT in another, so header order is NOT text order, and
// "greatest base <= pc" hands a libsystem_malloc pc to whichever unrelated image
// has the nearest lower header — which is how a `_xzm_free` leaf was reported as
// `task_get_special_port`. Emit the real executable range so the offline
// symbolizer can decide ownership by containment.
static const uint8_t *image_text_range(const struct mach_header *mh,
                                       unsigned long *size) {
  *size = 0;
  if (mh == NULL) {
    return NULL;
  }
#if defined(__LP64__)
  return getsegmentdata((const struct mach_header_64 *)mh, SEG_TEXT, size);
#else
  return getsegmentdata(mh, SEG_TEXT, size);
#endif
}

// The mach-o slice this image was mapped as, as `atos -arch` spells it.
static const char *image_arch(const struct mach_header *mh) {
#if defined(__LP64__)
  const struct mach_header_64 *h = (const struct mach_header_64 *)mh;
#else
  const struct mach_header *h = mh;
#endif
  if (h->cputype != CPU_TYPE_ARM64) {
    return h->cputype == CPU_TYPE_X86_64 ? "x86_64" : "unknown";
  }
  return (h->cpusubtype & ~CPU_SUBTYPE_MASK) == CPU_SUBTYPE_ARM64E ? "arm64e"
                                                                  : "arm64";
}

// One image row. Shared by the dyld-registered loop and by dyld itself, which
// is NOT in that list.
static void dump_one_image(FILE *f, const char *sep, const char *path,
                           const struct mach_header *mh, uint64_t slide) {
  fprintf(f, "%s{\"path\":", sep);
  osp_prof_json_string(f, path);
  unsigned long text_size = 0;
  const uint8_t *text = image_text_range(mh, &text_size);
  fprintf(f,
          ",\"base\":%" PRIu64 ",\"slide\":%" PRIu64 ",\"text\":%" PRIu64
          ",\"text_size\":%" PRIu64 ",\"arch\":\"%s\"}",
          (uint64_t)(uintptr_t)mh, slide, (uint64_t)(uintptr_t)text,
          (uint64_t)text_size, image_arch(mh));
}

// dyld maps itself and every frame of process startup lives in it, but
// `_dyld_image_count()` does not report it — the list it walks is the one dyld
// keeps of what IT loaded. Without this row those frames have no owning image,
// so they can only be printed as raw hex [PROF-SYMBOLIZE-OFFLINE]. The loader's
// base comes from the task's dyld info, which is the only published route to it.
static const struct mach_header *dyld_image(void) {
  struct task_dyld_info info;
  mach_msg_type_number_t count = TASK_DYLD_INFO_COUNT;
  if (task_info(mach_task_self(), TASK_DYLD_INFO, (task_info_t)&info, &count) !=
      KERN_SUCCESS) {
    return NULL;
  }
  const struct dyld_all_image_infos *all =
      (const struct dyld_all_image_infos *)(uintptr_t)info.all_image_info_addr;
  return all ? all->dyldImageLoadAddress : NULL;
}

void osp_prof_write_images(FILE *f) {
  uint32_t count = _dyld_image_count();
  for (uint32_t i = 0; i < count; i++) {
    dump_one_image(f, i == 0 ? "" : ",", _dyld_get_image_name(i),
                   _dyld_get_image_header(i),
                   (uint64_t)_dyld_get_image_vmaddr_slide(i));
  }
  const struct mach_header *loader = dyld_image();
  if (loader) {
    dump_one_image(f, count == 0 ? "" : ",", "/usr/lib/dyld", loader, 0);
  }
}

uint32_t osp_prof_image_count(void) {
  return _dyld_image_count() + (dyld_image() ? 1U : 0U);
}

void osp_prof_write_exe(FILE *f) {
  char buf[4096];
  uint32_t size = sizeof(buf);
  osp_prof_json_string(f, _NSGetExecutablePath(buf, &size) == 0 ? buf : "");
}
#else
// The separator state MUST travel with the callback, not live in a
// function-local static: `dl_iterate_phdr` is called once per dump, and a
// static kept "already wrote one" set across dumps. Every capture after the
// first then opened its array with the separator — `"images":[,{...}` — which
// is not JSON at all, so nothing downstream could read the image list it needs
// to symbolize an address. macOS is unaffected because its loop derives the
// separator from the index [PROF-RAW-FORMAT].
typedef struct {
  FILE *f;
  int written;
} OspImageDump;

// The ELF twin of image_text_range: the span of this image's EXECUTABLE PT_LOAD
// segments. ELF has no shared cache, so header order does hold here — but the
// symbolizer picks containment first on every platform, and a range it can trust
// beats an ordering it has to assume.
static uint64_t exec_span(const struct dl_phdr_info *info, uint64_t *start) {
  uint64_t lo = 0, hi = 0;
  int seen = 0;
  for (uint16_t j = 0; j < info->dlpi_phnum; j++) {
    const ElfW(Phdr) *ph = &info->dlpi_phdr[j];
    if (ph->p_type != PT_LOAD || (ph->p_flags & PF_X) == 0) {
      continue;
    }
    uint64_t seg_lo = (uint64_t)info->dlpi_addr + (uint64_t)ph->p_vaddr;
    uint64_t seg_hi = seg_lo + (uint64_t)ph->p_memsz;
    lo = (!seen || seg_lo < lo) ? seg_lo : lo;
    hi = (!seen || seg_hi > hi) ? seg_hi : hi;
    seen = 1;
  }
  *start = lo;
  return hi > lo ? hi - lo : 0;
}

static int dump_image_cb(struct dl_phdr_info *info, size_t size, void *data) {
  (void)size;
  OspImageDump *dump = data;
  fprintf(dump->f, "%s{\"path\":", dump->written ? "," : "");
  dump->written = 1;
  osp_prof_json_string(dump->f, info->dlpi_name ? info->dlpi_name : "");
  uint64_t text = 0;
  uint64_t text_size = exec_span(info, &text);
  fprintf(dump->f,
          ",\"base\":%" PRIu64 ",\"slide\":%" PRIu64 ",\"text\":%" PRIu64
          ",\"text_size\":%" PRIu64 "}",
          (uint64_t)info->dlpi_addr, (uint64_t)info->dlpi_addr, text, text_size);
  return 0;
}

void osp_prof_write_images(FILE *f) {
  OspImageDump dump = {f, 0};
  dl_iterate_phdr(dump_image_cb, &dump);
}

static int count_image_cb(struct dl_phdr_info *info, size_t size, void *data) {
  (void)info;
  (void)size;
  uint32_t *count = data;
  (*count)++;
  return 0;
}

uint32_t osp_prof_image_count(void) {
  uint32_t count = 0;
  dl_iterate_phdr(count_image_cb, &count);
  return count;
}

void osp_prof_write_exe(FILE *f) {
  char buf[4096];
  ssize_t n = readlink("/proc/self/exe", buf, sizeof(buf) - 1);
  buf[n > 0 ? n : 0] = '\0';
  osp_prof_json_string(f, buf);
}
#endif

uint32_t osp_prof_write_threads(FILE *f, uint32_t from) {
  // Serialize against a late registration's row append.
  osp_prof_registry_lock();
  uint32_t count = osp_prof_row_count();
  for (uint32_t i = from; i < count; i++) {
    fprintf(f, "%s{\"fiber\":%lld,\"label\":", i == from ? "" : ",",
            (long long)osp_prof_row_fiber(i));
    osp_prof_json_string(f, osp_prof_row_label(i));
    fputc('}', f);
  }
  osp_prof_registry_unlock();
  return count;
}

size_t osp_prof_write_stacks(FILE *f, size_t from) {
  size_t count = osp_prof_stack_count();
  for (size_t i = from; i < count; i++) {
    uint32_t len = 0;
    const uint64_t *pcs = osp_prof_stack_pcs(i, &len);
    fprintf(f, "%s[", i == from ? "" : ",");
    for (uint32_t j = 0; j < len; j++) {
      fprintf(f, "%s%" PRIu64, j == 0 ? "" : ",", pcs[j]);
    }
    fputc(']', f);
  }
  return count;
}

void osp_prof_write_samples(FILE *f, bool with_tick) {
  size_t count = osp_prof_sample_count();
  for (size_t i = 0; i < count; i++) {
    OspProfSample s = osp_prof_sample_at(i);
    fprintf(f, "%s[%" PRIu64 ",%u,%u,%u", i == 0 ? "" : ",", s.t_ns,
            s.thread, s.stack, s.state);
    if (with_tick) {
      fprintf(f, ",%u", s.tick);
    }
    fputc(']', f);
  }
}

const char *osp_prof_platform(void) {
#if defined(__APPLE__) && defined(__aarch64__)
  return "macos-arm64";
#elif defined(__APPLE__)
  return "macos-x86_64";
#elif defined(__aarch64__)
  return "linux-arm64";
#else
  return "linux-x86_64";
#endif
}

void osp_prof_write_raw(FILE *f) {
  fprintf(f,
          "{\"version\":1,\"pid\":%lld,\"rate_hz\":%u,\"platform\":\"%s\","
          "\"start_unix_ns\":%" PRIu64 ",\"end_unix_ns\":%" PRIu64
          ",\"dropped\":%" PRIu64 ",\"exe\":",
          (long long)getpid(), osp_prof_rate_hz(), osp_prof_platform(),
          osp_prof_session_start_unix_ns(), osp_prof_unix_ns(),
          osp_prof_dropped());
  osp_prof_write_exe(f);
  fprintf(f, ",\"images\":[");
  osp_prof_write_images(f);
  fprintf(f, "],\"threads\":[");
  (void)osp_prof_write_threads(f, 0);
  fprintf(f, "],\"stacks\":[");
  (void)osp_prof_write_stacks(f, 0);
  fprintf(f, "],\"samples\":[");
  osp_prof_write_samples(f, false);
  fprintf(f, "]}\n");
}

#else

typedef int osp_prof_dump_unused; // profiling is POSIX-only

#endif // !_WIN32 && !__wasm__
