// The casting player's device browse on Windows [CAST-DESIGN-NATIVE]. Windows has no dns-sd, so
// this program browses _googlecast._tcp with DnsServiceBrowse, resolves each instance with
// DnsServiceResolve and prints the `dns-sd -Z` SRV and TXT lines chromecast::Discovery.browse
// reads, then stops by itself. The casting app names it with `--browse` and the host runs it.
// Build: gcc -O2 cast-browse.c -o cast-browse.exe -ldnsapi
#define UNICODE
#define _UNICODE
#include <windows.h>
#include <windns.h>
#include <stdio.h>
#include <string.h>

#define BROWSE_MS 3000
#define MAX_FOUND 64
#define TEXT_BYTES 1024

static CRITICAL_SECTION lock;
static WCHAR found[MAX_FOUND][DNS_MAX_NAME_BUFFER_LENGTH];
static DNS_SERVICE_CANCEL resolving[MAX_FOUND];
static int found_count;

// `text` as UTF-8 in `out`, "" when it is missing or does not fit.
static void utf8(PCWSTR text, char *out, int size) {
  if (!text || WideCharToMultiByte(CP_UTF8, 0, text, -1, out, size, NULL, NULL) == 0) {
    out[0] = 0;
  }
}

// The instance's record key as dns-sd prints it: a space as \032, without ".local".
static void record_key(PCWSTR instance, char *out, int size) {
  char name[TEXT_BYTES];
  utf8(instance, name, sizeof name);
  size_t n = strlen(name);
  if (n > 6 && _stricmp(name + n - 6, ".local") == 0) {
    name[n - 6] = 0;
  }
  int at = 0;
  for (const char *c = name; *c && at < size - 5; c++) {
    if (*c == ' ') {
      memcpy(out + at, "\\032", 4);
      at += 4;
    } else {
      out[at++] = *c;
    }
  }
  out[at] = 0;
}

// Where to reach the device: its IPv4 address when resolved, else its host name.
static void reach(PDNS_SERVICE_INSTANCE instance, char *out, int size) {
  if (instance->ip4Address) {
    const BYTE *b = (const BYTE *)instance->ip4Address;
    snprintf(out, size, "%u.%u.%u.%u", b[0], b[1], b[2], b[3]);
  } else {
    utf8(instance->pszHostName, out, size);
  }
}

// The instance's friendly name, its `fn` property, or "".
static void friendly(PDNS_SERVICE_INSTANCE instance, char *out, int size) {
  out[0] = 0;
  for (DWORD i = 0; i < instance->dwPropertyCount; i++) {
    if (instance->keys[i] && wcscmp(instance->keys[i], L"fn") == 0) {
      utf8(instance->values[i], out, size);
    }
  }
}

// The SRV and TXT lines of one resolved instance, printed whole under the lock.
static void print_instance(PDNS_SERVICE_INSTANCE instance) {
  char key[TEXT_BYTES], host[TEXT_BYTES], name[TEXT_BYTES];
  record_key(instance->pszInstanceName, key, sizeof key);
  reach(instance, host, sizeof host);
  friendly(instance, name, sizeof name);
  if (!key[0] || !host[0] || !name[0] || strchr(name, '"')) {
    return;
  }
  EnterCriticalSection(&lock);
  printf("%s SRV %u %u %u %s ;\n", key, instance->wPriority, instance->wWeight, instance->wPort, host);
  printf("%s TXT \"fn=%s\"\n", key, name);
  fflush(stdout);
  LeaveCriticalSection(&lock);
}

static VOID WINAPI resolved(DWORD status, PVOID context, PDNS_SERVICE_INSTANCE instance) {
  (void)context;
  if (status == ERROR_SUCCESS && instance) {
    print_instance(instance);
  }
  if (instance) {
    DnsServiceFreeInstance(instance);
  }
}

// The slot for a newly seen instance name, or -1 when it was seen already or no slot is left.
static int remember(PCWSTR name) {
  EnterCriticalSection(&lock);
  int slot = found_count < MAX_FOUND ? found_count : -1;
  for (int i = 0; i < found_count; i++) {
    if (_wcsicmp(found[i], name) == 0) {
      slot = -1;
    }
  }
  if (slot >= 0) {
    wcsncpy(found[slot], name, DNS_MAX_NAME_BUFFER_LENGTH - 1);
    found_count++;
  }
  LeaveCriticalSection(&lock);
  return slot;
}

// Resolves an instance the browse named for the first time.
static void resolve(PCWSTR name) {
  int slot = remember(name);
  if (slot < 0) {
    return;
  }
  DNS_SERVICE_RESOLVE_REQUEST request = {0};
  request.Version = DNS_QUERY_REQUEST_VERSION1;
  request.QueryName = found[slot];
  request.pResolveCompletionCallback = resolved;
  DnsServiceResolve(&request, &resolving[slot]);
}

static VOID WINAPI browsed(DWORD status, PVOID context, PDNS_RECORD records) {
  (void)context;
  for (PDNS_RECORD r = records; status == ERROR_SUCCESS && r; r = r->pNext) {
    if (r->wType == DNS_TYPE_PTR && r->Data.PTR.pNameHost) {
      resolve(r->Data.PTR.pNameHost);
    }
  }
  if (records) {
    DnsRecordListFree(records, DnsFreeRecordList);
  }
}

// The browse's deadline, kept on its own thread: DnsServiceBrowse and its cancel can block on a
// machine with no multicast route, and the host's run must still end with the lines found so far.
static DWORD WINAPI stop_browsing(LPVOID unused) {
  (void)unused;
  Sleep(BROWSE_MS);
  // A resolve still running is abandoned: the lock keeps a line from being cut short.
  EnterCriticalSection(&lock);
  fflush(stdout);
  // Not ExitProcess: its DLL teardown can wait on the browse that is still blocked.
  TerminateProcess(GetCurrentProcess(), 0);
  return 0;
}

int main(void) {
  InitializeCriticalSection(&lock);
  if (CreateThread(NULL, 0, stop_browsing, NULL, 0, NULL) == NULL) {
    fprintf(stderr, "CreateThread failed: %lu\n", GetLastError());
    return 1;
  }
  DNS_SERVICE_BROWSE_REQUEST request = {0};
  DNS_SERVICE_CANCEL browsing = {0};
  request.Version = DNS_QUERY_REQUEST_VERSION1;
  request.QueryName = L"_googlecast._tcp.local";
  request.pBrowseCallback = browsed;
  DNS_STATUS status = DnsServiceBrowse(&request, &browsing);
  if (status != DNS_REQUEST_PENDING) {
    fprintf(stderr, "DnsServiceBrowse failed: %ld\n", (long)status);
    return 1;
  }
  Sleep(INFINITE);
  return 0;
}
