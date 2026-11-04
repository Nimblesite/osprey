// Generic FFI pointer helpers for the Osprey foreign-function interface.
//
// These let Osprey call C APIs that use OUT-PARAMETERS (e.g. sqlite3_open's
// `sqlite3 **` or sqlite3_prepare_v2's `sqlite3_stmt **`) without any
// library-specific runtime code: allocate a pointer cell, pass it to the C call,
// then read the stored pointer back. They are deliberately library-agnostic —
// every C binding (SQLite, libpq, compression, ...) reuses the same primitives.
//
// All return/accept an opaque `void*`, which the Osprey `Ptr` type lowers to.

#include <stdint.h>
#include <stdlib.h>

// osprey_ffi_cell allocates a zeroed pointer-sized cell for use as a C
// out-parameter. The caller owns it and must release it with osprey_ffi_free.
void *osprey_ffi_cell(void) { return calloc(1, sizeof(void *)); }

// osprey_ffi_deref reads the pointer stored in a cell (i.e. *(void**)cell).
void *osprey_ffi_deref(void *cell) {
  if (cell == NULL) {
    return NULL;
  }

  return *(void **)cell;
}

// osprey_ffi_free releases a cell allocated by osprey_ffi_cell.
int64_t osprey_ffi_free(void *cell) {
  free(cell);

  return 0;
}

// osprey_ffi_null returns a NULL pointer for optional C arguments.
void *osprey_ffi_null(void) { return NULL; }

// osprey_ffi_transient returns SQLite's SQLITE_TRANSIENT sentinel ((void*)-1),
// instructing a C API to copy a passed buffer rather than alias it.
void *osprey_ffi_transient(void) { return (void *)(intptr_t)-1; }

// osprey_ffi_is_null reports whether a pointer is NULL (1) or not (0).
int64_t osprey_ffi_is_null(void *ptr) { return ptr == NULL ? 1 : 0; }
