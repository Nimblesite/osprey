// Browser host bridge for Osprey's wasm target. [WASM-WEB-ABI]
//
// React (or another DOM framework) stays in JavaScript. Osprey crosses the
// wasm boundary with coarse-grained, NUL-terminated UTF-8/JSON messages rather
// than making one host call per DOM node. This unit is intentionally present
// only in WASM_RT_SRC; native builds have no `osprey_web` import module.

#ifndef __wasm__
#error "web_runtime.c is only supported by the WebAssembly runtime"
#endif

#include <stdint.h>

__attribute__((import_module("osprey_web"), import_name("render")))
extern void osprey_web_host_render(const char *message);

__attribute__((import_module("osprey_web"), import_name("command")))
extern void osprey_web_host_command(const char *message);

// Osprey declarations:
//   extern fn osprey_web_render(message: string) -> int
//   extern fn osprey_web_command(message: string) -> int
// The host imports are notifications, while the Osprey-facing status is i64.
int64_t osprey_web_render(char *message) {
  osprey_web_host_render(message);
  return 0;
}

int64_t osprey_web_command(char *message) {
  osprey_web_host_command(message);
  return 0;
}
