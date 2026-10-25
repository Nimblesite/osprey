//! Per-allocation layout metadata for `@osp_alloc_tagged` — the kind + word
//! bitmask the ARC backend stores in the object header. Implements
//! [GC-ARC-PERCEUS] (docs/specs/compiler/0018-MemoryManagement.md): codegen knows the
//! exact layout at each alloc site, so it passes the layout down instead of
//! relying on a pointers-first `scan_fsize` convention the existing ABIs
//! (tag-first records, `Result`'s trailing errmsg, `HttpResponse`'s C layout)
//! would violate. Non-counting backends ignore the word.
//!
//! Encoding (must match `compiler/runtime/memory_arc.c`): low 8 bits a kind,
//! upper 56 a bitmask — bit `8 + i` set ⇒ the pointer-stride word at body
//! offset `pointer_bytes * i` is a managed pointer the drop walk releases.
//! The stride is the target's pointer size: 8 natively, 4 on wasm32.

use crate::llty::LType;

/// Pointer size of a native 64-bit target.
pub(crate) const NATIVE_POINTER_BYTES: u64 = 8;
/// Pointer size of wasm32 (and every other ILP32 target).
pub(crate) const WASM32_POINTER_BYTES: u64 = 4;

/// Opaque bytes: no children to release. The default for plain `osp_alloc`.
pub(crate) const KIND_RAW: i64 = 0;
/// Children live at the masked word offsets.
const KIND_MASK: i64 = 1;
/// `{ i64 len, i8* data }` with pointer elements: release `data[0..len)`,
/// then `data`.
const KIND_LIST_HDR_PTR: i64 = 2;
/// `{ i64 len, i8* data }` with scalar elements: release `data` only.
const KIND_LIST_HDR_SCALAR: i64 = 3;
/// A `KIND_MASK` whose every masked child is PROVEN to be an ARC body or
/// NULL (all fields are declared-union values, which only constructors can
/// produce): the drop walk reads child headers directly, skipping the
/// registry probe that dominates deep-structure drops.
const KIND_MASK_DIRECT: i64 = 5;

/// Highest word index the 56-bit mask can name.
const MASK_MAX_WORD: u64 = 55;

/// One struct field as the layout calculator sees it. Pointer-typed fields are
/// split by whether the ARC drop should release them: capture cells' leading
/// function pointer and other code/foreign pointers are `PtrOpaque`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetaField {
    /// 8-byte scalar (`i64` / `double`). Never marked — a boxed pointer in an
    /// `i64` slot stays unmarked (leak-safe): a false release could corrupt,
    /// while a missed release can only leak.
    Word,
    /// An 8-byte physical slot holding a managed pointer that is boxed into it
    /// (`Result`'s always-`i64` payload, an `any` box's `i64` payload). The
    /// slot stays 8 bytes on every target, unlike `PtrManaged`, but its child
    /// is a heap pointer the drop walk must release.
    ManagedWord,
    /// Pointer-size managed pointer — the drop walk releases it.
    PtrManaged,
    /// Pointer-size managed pointer PROVEN to be an ARC body or NULL (a
    /// declared-union field: only constructors mint such values, and no
    /// extern claims to return the union). When every masked field is
    /// `PtrDirect` the struct takes `KIND_MASK_DIRECT` and its drop walk
    /// skips the per-child registry probe.
    PtrDirect,
    /// Pointer-size pointer the drop walk must NOT release (code pointer,
    /// foreign).
    PtrOpaque,
    /// 1-byte field (`i1` / `i8`).
    Byte,
    /// 4-byte field (`i32`).
    Half,
}

impl MetaField {
    /// Size and natural alignment of this field on a target whose pointers are
    /// `pointer_bytes` wide. `ManagedWord` is a physical `i64` on every target.
    pub(crate) fn size_align(self, pointer_bytes: u64) -> u64 {
        match self {
            MetaField::Word | MetaField::ManagedWord => 8,
            MetaField::PtrManaged | MetaField::PtrDirect | MetaField::PtrOpaque => pointer_bytes,
            MetaField::Byte => 1,
            MetaField::Half => 4,
        }
    }

    /// Whether the drop walk releases this field's contents.
    fn is_managed(self) -> bool {
        matches!(
            self,
            MetaField::ManagedWord | MetaField::PtrManaged | MetaField::PtrDirect
        )
    }

    /// The layout field for a value of LLVM type `lty`: pointers are managed
    /// (`Str`/`Ptr` values are heap handles; rodata/foreign pointers are
    /// covered by the runtime's registry probe-miss).
    pub(crate) fn of_lty(lty: LType) -> MetaField {
        match lty {
            // An erased-`any` box is an ordinary tagged allocation, so a slot
            // holding one is managed like any other heap handle
            // ([`LType::is_managed_ptr`] is the same classification for
            // callers that only need the boolean).
            LType::Str | LType::Ptr | LType::Any => MetaField::PtrManaged,
            LType::I1 => MetaField::Byte,
            LType::I32 => MetaField::Half,
            LType::I64 | LType::Double => MetaField::Word,
        }
    }

    /// The layout field for a textual LLVM slot type (closure / effect-env
    /// cells spell their slots as strings): any pointer spelling is managed.
    pub(crate) fn of_slot_ty(slot_ty: &str) -> MetaField {
        match slot_ty {
            "i1" | "i8" => MetaField::Byte,
            "i32" => MetaField::Half,
            "i64" | "double" => MetaField::Word,
            s if s.ends_with('*') => MetaField::PtrManaged,
            _ => MetaField::Word,
        }
    }
}

/// Physical field offsets shared by allocation metadata and native DWARF.
pub(crate) fn field_offsets(fields: &[MetaField]) -> impl Iterator<Item = (u64, u64)> + '_ {
    field_offsets_for(fields, NATIVE_POINTER_BYTES)
}

/// Physical field offsets on a target whose pointers are `pointer_bytes` wide.
pub(crate) fn field_offsets_for(
    fields: &[MetaField],
    pointer_bytes: u64,
) -> impl Iterator<Item = (u64, u64)> + '_ {
    fields.iter().scan(0u64, move |offset, field| {
        let size = field.size_align(pointer_bytes);
        let start = offset.div_ceil(size) * size;
        *offset = start + size;
        Some((start, size))
    })
}

/// Pointer size for a target: 4 bytes when it is wasm32, else native 8.
pub(crate) fn pointer_bytes_for(wasm32: bool) -> u64 {
    if wasm32 {
        WASM32_POINTER_BYTES
    } else {
        NATIVE_POINTER_BYTES
    }
}

impl crate::builder::Codegen {
    /// The `@osp_alloc_tagged` layout word for `fields`, computed for THIS
    /// target's pointer width. Every production struct-meta call site goes
    /// through here so wasm32 masks stay physically exact.
    pub(crate) fn struct_meta(&self, fields: &[MetaField]) -> i64 {
        struct_meta_for(fields, self.pointer_bytes)
    }
}

/// The native-64-bit [`struct_meta_for`]; the layout tests pin its encoding.
#[cfg(test)]
pub(crate) fn struct_meta(fields: &[MetaField]) -> i64 {
    struct_meta_for(fields, NATIVE_POINTER_BYTES)
}

/// The meta word for a struct laid out from `fields` in order, natural
/// alignment (LLVM's rules for this field set). Falls back to `KIND_RAW`
/// (leak-safe, never corrupting) when a managed pointer lands beyond the
/// mask's reach. All-`PtrDirect` masks upgrade to `KIND_MASK_DIRECT`; one
/// unproven field keeps the whole struct on the probing `KIND_MASK`. The mask
/// bit of a field is its actual byte offset divided by the target's pointer
/// stride, so the drop walk can step pointer-sized words on wasm32 too.
pub(crate) fn struct_meta_for(fields: &[MetaField], pointer_bytes: u64) -> i64 {
    let mut mask: u64 = 0;
    let mut all_direct = true;
    for (f, (off, _)) in fields.iter().zip(field_offsets_for(fields, pointer_bytes)) {
        if f.is_managed() {
            let word = off / pointer_bytes;
            if word > MASK_MAX_WORD {
                return KIND_RAW;
            }
            mask |= 1u64 << word;
            all_direct &= *f == MetaField::PtrDirect;
        }
    }
    if mask == 0 {
        KIND_RAW
    } else {
        let kind = if all_direct {
            KIND_MASK_DIRECT
        } else {
            KIND_MASK
        };
        // Bit-preserving: word 55 lands on bit 63 (the sign bit); the runtime
        // recovers the mask via an unsigned cast (memory_arc.c OSP_ARC_MASK_BITS).
        i64::from_ne_bytes(((mask << 8) | u64::from_ne_bytes(kind.to_ne_bytes())).to_ne_bytes())
    }
}

/// The meta word for a flat list-literal header `{ i64 len, i8* data }`.
pub(crate) fn list_hdr_meta(elems_are_ptrs: bool) -> i64 {
    if elems_are_ptrs {
        KIND_LIST_HDR_PTR
    } else {
        KIND_LIST_HDR_SCALAR
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask_words(meta: i64) -> Vec<u64> {
        // Same bit-preserving recovery the runtime uses (OSP_ARC_MASK_BITS).
        let bits = u64::from_ne_bytes(meta.to_ne_bytes()) >> 8;
        (0..=MASK_MAX_WORD)
            .filter(|w| bits & (1 << w) != 0)
            .collect()
    }

    #[test]
    fn result_block_masks_depend_on_payload() {
        // { i64, i8, i8* }: payload word 0 unmarked, errmsg at offset 16.
        let m = struct_meta(&[MetaField::Word, MetaField::Byte, MetaField::PtrManaged]);
        assert_eq!(m & 0xFF, KIND_MASK);
        assert_eq!(mask_words(m), vec![2]);
        // { i8*, i8, i8* }: payload word 0 marked too.
        let m = struct_meta(&[
            MetaField::PtrManaged,
            MetaField::Byte,
            MetaField::PtrManaged,
        ]);
        assert_eq!(mask_words(m), vec![0, 2]);
        // { i1, i8, i8* }: i1 packs to one byte, errmsg realigns to offset 8.
        let m = struct_meta(&[MetaField::Byte, MetaField::Byte, MetaField::PtrManaged]);
        assert_eq!(mask_words(m), vec![1]);
    }

    #[test]
    fn http_response_marks_words_1_2_5() {
        // { i64, i8*, i8*, i64, i8, i8* } — the fixed C ABI (http_shared.h).
        let m = struct_meta(&[
            MetaField::Word,
            MetaField::PtrManaged,
            MetaField::PtrManaged,
            MetaField::Word,
            MetaField::Byte,
            MetaField::PtrManaged,
        ]);
        assert_eq!(mask_words(m), vec![1, 2, 5]);
    }

    #[test]
    fn scalar_structs_and_opaque_pointers_are_raw() {
        assert_eq!(struct_meta(&[MetaField::Word, MetaField::Word]), KIND_RAW);
        // Closure cell with no managed captures: fnptr is opaque.
        assert_eq!(
            struct_meta(&[MetaField::PtrOpaque, MetaField::Word]),
            KIND_RAW
        );
    }

    #[test]
    fn all_proven_fields_upgrade_to_mask_direct() {
        // { i64 tag, Tree left, Tree right, i64 v } — the binarytrees Node.
        let m = struct_meta(&[
            MetaField::Word,
            MetaField::PtrDirect,
            MetaField::PtrDirect,
            MetaField::Word,
        ]);
        assert_eq!(m & 0xFF, KIND_MASK_DIRECT);
        assert_eq!(mask_words(m), vec![1, 2]);
        // One unproven (string) field demotes the whole struct to probing MASK.
        let m = struct_meta(&[MetaField::Word, MetaField::PtrDirect, MetaField::PtrManaged]);
        assert_eq!(m & 0xFF, KIND_MASK);
        assert_eq!(mask_words(m), vec![1, 2]);
    }

    #[test]
    fn oversized_structs_fall_back_to_raw() {
        let mut fields = vec![MetaField::Word; 56];
        fields.push(MetaField::PtrManaged);
        assert_eq!(struct_meta(&fields), KIND_RAW);
    }
}
