# osprey: flavor=python
# osprey: namespace=safetensors
# osprey: module=Dtype
"""The safetensors element dtypes this port decodes and encodes, their byte
sizes and their alignment order. The order is the Rust crate's `Dtype` enum
declaration order (increasing alignment), which `save` sorts descending to lay
tensors out like `safetensors.torch.save`.
"""

__all__ = ["dtype_size", "dtype_rank", "is_float_dtype", "is_int_dtype", "dtype_order"]


def dtype_size(dtype):
    """Bytes per element of a safetensors dtype string (BOOL..U64).

    # Examples
    ```osprey
    from safetensors import Dtype
    f32 = "F32"
    b = "BOOL"
    unknown = "Q9"
    print(f"[{Dtype.dtype_size(f32)}] [{Dtype.dtype_size(b)}] [{Dtype.dtype_size(unknown)}]")
    ```
    ```output
    [4] [1] [0]
    ```
    """
    match dtype:
        case "BOOL":
            return 1
        case "U8":
            return 1
        case "I8":
            return 1
        case "I16":
            return 2
        case "U16":
            return 2
        case "F16":
            return 2
        case "BF16":
            return 2
        case "I32":
            return 4
        case "U32":
            return 4
        case "F32":
            return 4
        case "F64":
            return 8
        case "I64":
            return 8
        case "U64":
            return 8
        case _:
            return 0


def dtype_order():
    """The 13 dtypes in increasing alignment order.

    # Examples
    ```osprey
    from safetensors import Dtype
    order = Dtype.dtype_order()
    print(f"[{toString(order)}] [{len(order)}]")
    ```
    ```output
    [["BOOL", "U8", "I8", "I16", "U16", "F16", "BF16", "I32", "U32", "F32", "F64", "I64", "U64"]] [13]
    ```
    """
    return ["BOOL", "U8", "I8", "I16", "U16", "F16", "BF16", "I32", "U32", "F32", "F64", "I64", "U64"]


def dtype_rank(dtype):
    """The alignment index of `dtype` (0..12), or -1 when unknown.

    # Examples
    ```osprey
    from safetensors import Dtype
    f32 = "F32"
    b = "BOOL"
    unknown = "Q9"
    print(f"[{Dtype.dtype_rank(f32)}] [{Dtype.dtype_rank(b)}] [{Dtype.dtype_rank(unknown)}]")
    ```
    ```output
    [9] [0] [-1]
    ```
    """
    return fold(range(13), -1, lambda found, i: i if found < 0 and unwrap_or(listGet(dtype_order(), i), "") == dtype else found)


def is_float_dtype(dtype):
    """Whether `dtype` decodes to floats (F16 BF16 F32 F64).

    # Examples
    ```osprey
    from safetensors import Dtype
    f32 = "F32"
    u8 = "U8"
    unknown = "Q9"
    print(f"[{Dtype.is_float_dtype(f32)}] [{Dtype.is_float_dtype(u8)}] [{Dtype.is_float_dtype(unknown)}]")
    ```
    ```output
    [true] [false] [false]
    ```
    """
    match dtype:
        case "F16":
            return True
        case "BF16":
            return True
        case "F32":
            return True
        case "F64":
            return True
        case _:
            return False


def is_int_dtype(dtype):
    """Whether `dtype` decodes to ints (BOOL U8 I8 I16 U16 I32 U32 I64 U64).

    # Examples
    ```osprey
    from safetensors import Dtype
    u8 = "U8"
    f32 = "F32"
    unknown = "Q9"
    print(f"[{Dtype.is_int_dtype(u8)}] [{Dtype.is_int_dtype(f32)}] [{Dtype.is_int_dtype(unknown)}]")
    ```
    ```output
    [true] [false] [true]
    ```
    """
    return not is_float_dtype(dtype)
