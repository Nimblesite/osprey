# osprey: flavor=python
# osprey: namespace=numpy
# osprey: module=Device
"""The GPU interpretation of `Storage.Buffer`: every operation is a kernel, a
product or a transfer of the GPU surface — the CuPy equivalent, with device
placement decided by the handler rather than a duplicate API. Kernels run on
Metal or OpenCL when `OSPREY_GPU` selects a device and on the host backend
otherwise, with the same bytes either way [GPU-BACKEND-HOST]. Implements
[PORT-DEVICE], [GPU-DEVICE], [FLAVOR-PYTHON-GPU].
"""
from numpy import Storage

__all__ = ["on_gpu"]


def product_of(result):
    """The GPU product as `Storage.Buffer.mat_mul` answers it: the buffer, or
    the runtime's reason as text."""
    match result:
        case Success(value=value):
            return Success(value=value)
        case Error(message=message):
            return Error(message=toString(message))


@static_handler(Storage.Buffer[GpuBuffer[float]])
class on_gpu:
    """GPU buffers, and `device` reports which backend executes them."""
    def build(n, f):
        return gpuMap(gpuIota(n), f)

    def get(b, i):
        return unwrap_or(gpuGet(b, i), 0.0)

    def length(b):
        return gpuLength(b)

    def zip_with(a, b, f):
        return gpuZipWith(a, b, f)

    def reduce(b, seed, f):
        return gpuFold(b, seed, f)

    def mat_mul(a, b, rows, inner, cols):
        return product_of(gpuMatMul(a, b, rows, inner, cols))

    def from_list(xs):
        return toGpu(xs)

    def to_list(b):
        return fromGpu(b)

    def device():
        return gpuDevice()
