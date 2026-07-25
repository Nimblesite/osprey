/*
 * osprey_win_compat.h — Windows (MinGW-w64 / MSYS2 UCRT64) compatibility shims.
 *
 * Included only on Windows (`_WIN32`). The POSIX
 * build never sees this header. Kept deliberately small:
 *
 *   Phase 1 (core language): just <windows.h>. The process runtime is stubbed
 *           and the socket runtime is not built, so nothing more is needed yet.
 *   Phase 2 (HTTP/WebSocket): adds the Winsock2 surface (see the SOCKETS block).
 *   Phase 3 (process):        the system runtime uses the Win32 process APIs
 *           that <windows.h> already provides.
 *
 * winpthreads from MSYS2 UCRT64 provides <pthread.h> and -lpthread
 * unchanged, so fiber/effects code needs no shim here.
 */
#ifndef OSPREY_WIN_COMPAT_H
#define OSPREY_WIN_COMPAT_H

#ifdef _WIN32

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif

/* winsock2.h + ws2tcpip.h MUST come before windows.h, otherwise windows.h pulls
 * in the legacy winsock.h and the two conflict. */
#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>

/* The HTTP/WebSocket runtime is written against BSD sockets. On Winsock the
 * functions are largely call-compatible (socket/bind/listen/accept/recv/send/
 * select/setsockopt/htons/inet_addr/gethostbyname all exist in ws2_32), with
 * three differences the runtime hits:
 *   1. sockets are closed with closesocket(), not close().
 *   2. WSAStartup() must run once before any socket call (osprey_wsa_init).
 *   3. setsockopt()'s optval is `const char *`, handled with a cast at the
 *      call sites (portable: POSIX takes const void *).
 * http_shared.h includes this header, so the close->closesocket mapping is
 * scoped to the HTTP/WebSocket translation units, which only ever close
 * sockets (never file descriptors). */
#define close closesocket

/* POSIX <unistd.h> (and its sleep()) is not included on Windows — only this
 * header is. Map POSIX sleep(seconds) onto Win32 Sleep(milliseconds) so the
 * socket runtime's whole-second poll/shutdown loops compile. windows.h above
 * provides Sleep + DWORD; only the HTTP/WebSocket TUs see this. */
#define sleep(seconds) Sleep((DWORD)(seconds) * 1000U)

/* Initialise Winsock once per process. Implemented (and auto-run via a
 * constructor) in http_shared.c. Safe to call repeatedly. */
void osprey_wsa_init(void);

#endif /* _WIN32 */

#endif /* OSPREY_WIN_COMPAT_H */
