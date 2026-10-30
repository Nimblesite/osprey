# osprey: flavor=python
# osprey: namespace=gpt2
# osprey: module=Gpu
"""The GPU interpretation of `gpt2.Backends.Tensor`: every operation is a kernel, a
product or a loader of the GPU surface.
Implements [STAGE-BACKEND], [GPU-DEVICE], [FLAVOR-PYTHON-GPU].
"""
from gpt2.Backends import Tensor, hit, min_i

__all__ = ["on_gpu"]


def answered(result):
    """A GPU product or load, or the runtime's reason as text."""
    match result:
        case Success(value=value):
            return Success(value=value)
        case Error(message=message):
            return Error(message=toString(message))


@static_handler(Tensor[GpuBuffer[float]])
class on_gpu:
    """GPU buffers, and `device` reports which backend executes them. Kernels and
    products run on Metal or OpenCL when `OSPREY_GPU` selects a device."""
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

    def index_of(b, x):
        return gpuFold(gpuMap(gpuIota(gpuLength(b)), lambda i: hit(unwrap_or(gpuGet(b, i), 0.0), x, i, gpuLength(b))), gpuLength(b), min_i)

    def mat_mul(a, b, rows, inner, cols):
        return answered(gpuMatMul(a, b, rows, inner, cols))

    def from_list(xs):
        return toGpu(xs)

    def load(path, name):
        return answered(gpuLoadTensor(path, name))

    def device():
        return gpuDevice()
