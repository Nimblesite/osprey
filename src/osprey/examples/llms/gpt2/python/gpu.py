# osprey: flavor=python
# osprey: namespace=gpt2
# osprey: module=Gpu
"""The GPU interpretation of `gpt2.Backends.Tensor`: every operation is a kernel, a
product or a loader of the GPU surface.
Implements [STAGE-BACKEND], [GPU-DEVICE], [FLAVOR-PYTHON-GPU].
"""
from gpt2.Backends import Tensor, hit, min_i

__all__ = ["on_gpu"]


def then(result, next):
    """A GPU result, or the runtime's reason as text, ready for the next step."""
    match result:
        case Success(value=value):
            return next(value)
        case Error(message=message):
            return Error(message=toString(message))


def answered(result):
    """A GPU product, upload, kernel or load, or the runtime's reason as text."""
    return then(result, lambda value: Success(value=value))


@static_handler(Tensor[GpuBuffer[float32]])
class on_gpu:
    """GPU buffers, and `device` reports which backend executes them. Kernels and
    products run on Metal or OpenCL when `OSPREY_GPU` selects a device."""
    def build(n, f):
        return then(gpuIota(n), lambda value: answered(gpuMap(value, f)))

    def get(b, i):
        return unwrap_or(gpuGet(b, i), toFloat32(0.0))

    def length(b):
        return gpuLength(b)

    def zip_with(a, b, f):
        return answered(gpuZipWith(a, b, f))

    def reduce(b, seed, f):
        return answered(gpuFold(b, seed, f))

    def index_of(b, x):
        return then(gpuIota(gpuLength(b)), lambda value: then(
            gpuMap(value, lambda i: hit(unwrap_or(gpuGet(b, i), toFloat32(0.0)), x, i, gpuLength(b))),
            lambda mapped: answered(gpuFold(mapped, gpuLength(b), min_i))))

    def mat_mul(a, b, rows, inner, cols):
        return answered(gpuMatMul(a, b, rows, inner, cols))

    def from_list(xs):
        return answered(toGpu(xs))

    def load(path, name):
        return answered(gpuLoadTensor(path, name))

    def device():
        return gpuDevice()
