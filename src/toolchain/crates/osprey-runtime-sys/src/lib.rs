//! Raw FFI declarations for the Osprey C runtime (`src/toolchain/compiler/runtime/*.c`).
//!
//! The C is unchanged and keeps running its own memory-safety tests (`make
//! c-test`). This crate only exposes the symbols so the Rust codegen/runtime can
//! link them — the same C that compiled programs pull in via the static
//! archives (`libfiber_runtime.a` / `libhttp_runtime.a`).

use std::os::raw::c_void;

extern "C" {
    /// Allocate a zeroed pointer-sized cell for a C out-parameter.
    pub fn osprey_ffi_cell() -> *mut c_void;
    /// Read the pointer stored in a cell (`*(void**)cell`).
    pub fn osprey_ffi_deref(cell: *mut c_void) -> *mut c_void;
    /// Free a cell allocated by [`osprey_ffi_cell`].
    pub fn osprey_ffi_free(cell: *mut c_void);
    /// A null pointer constant for FFI sites.
    pub fn osprey_ffi_null() -> *mut c_void;
    /// `1` if the pointer is null, else `0`.
    pub fn osprey_ffi_is_null(ptr: *mut c_void) -> i64;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[expect(
        unsafe_code,
        reason = "drives the sealed FFI boundary this crate declares"
    )]
    fn cell_roundtrip_and_free() {
        unsafe {
            let cell = osprey_ffi_cell();
            assert!(!cell.is_null());
            // A freshly calloc'd cell dereferences to null.
            assert!(osprey_ffi_deref(cell).is_null());
            assert_eq!(osprey_ffi_is_null(osprey_ffi_null()), 1);
            osprey_ffi_free(cell);
        }
    }
}
