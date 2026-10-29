# osprey: flavor=python
# osprey: namespace=torch_tests
# osprey: module=Contracts
"""Exact small contracts for exported tensor helpers and autograd wrappers."""
from numpy import Array, Errors
from testing import Check
from torch import Autograd, Optim, Tensor

__all__ = ["run"]


def vector(name, a, want):
    return Check.close_all(name, Array.tolist(a), want, 0.000000000001, 0.0)


def tensor_binary_cases():
    a = Array.array1([2.0, 4.0])
    b = Array.array1([3.0])
    return [
        vector("tensor.badd", Tensor.badd(a, b), [5.0, 7.0]),
        vector("tensor.bsub", Tensor.bsub(a, b), [-1.0, 1.0]),
        vector("tensor.bmul", Tensor.bmul(a, b), [6.0, 12.0]),
        vector("tensor.bdiv", Tensor.bdiv(a, b), [2.0 / 3.0, 4.0 / 3.0]),
        vector("tensor.bmax", Tensor.bmax(a, b), [3.0, 4.0]),
        vector("tensor.bmin", Tensor.bmin(a, b), [2.0, 3.0]),
        vector("tensor.broadcast_to", Tensor.broadcast_to(b, [2]), [3.0, 3.0]),
    ]


def tensor_scalar_cases():
    a = Array.array1([2.0, 4.0])
    return [
        vector("tensor.scale", Tensor.scale(a, 3.0), [6.0, 12.0]),
        vector("tensor.add_scalar", Tensor.add_scalar(a, 3.0), [5.0, 7.0]),
        vector("tensor.sub_scalar", Tensor.sub_scalar(a, 3.0), [-1.0, 1.0]),
        vector("tensor.sub_from", Tensor.sub_from(3.0, a), [1.0, -1.0]),
        vector("tensor.div_scalar", Tensor.div_scalar(a, 2.0), [1.0, 2.0]),
        vector("tensor.max_scalar", Tensor.max_scalar(a, 3.0), [3.0, 4.0]),
        vector("tensor.gt_zero", Tensor.gt_zero(Array.array1([-1.0, 0.0, 1.0])), [0.0, 0.0, 1.0]),
        Check.equal("tensor.one_minus", Tensor.one_minus(0.25), 0.75),
        Check.equal("tensor.pow_f", Tensor.pow_f(2.0, 3), 8.0),
    ]


def tensor_layout_cases():
    a = Array.array2([[1.0, 2.0], [3.0, 4.0]])
    return [
        Check.equal("tensor.flat", Tensor.flat([2, 3]), 6),
        Check.ints_equal("tensor.strides", Tensor.strides([2, 3]), [3, 1]),
        Check.equal("tensor.src_offset", Tensor.src_offset(4, [2, 3], [1, 3]), 1),
        Check.ints_equal("tensor.bcast_shapes2", Tensor.bcast_shapes2([2, 1], [3]), [2, 3]),
        Check.equal("tensor.combine_op", Tensor.combine_op(1, 5.0, 2.0), 3.0),
        Check.close_all("tensor.combine_variants", toList(map(range(6), lambda op: Tensor.combine_op(op, 5.0, 2.0))), [7.0, 3.0, 10.0, 2.5, 5.0, 2.0], 0.0, 0.0),
        vector("tensor.sum_axes", Tensor.sum_axes(a, [1]), [3.0, 7.0]),
        vector("tensor.sum_to_size", Tensor.sum_to_size(a, [2, 1]), [3.0, 7.0]),
    ]


def tape_cases():
    a = Array.array1([2.0, 4.0])
    tape = Autograd.Tape(nodes=[])
    leaf = Autograd.leaf(tape, a)
    square = Autograd.square(leaf.tape, leaf.out)
    grads = Autograd.backward(square.tape, square.out, Array.ones_like(a))
    step = Autograd.Step(tape=square.tape, out=square.out)
    contribution = Autograd.Grad(pos=leaf.out, value=a)
    node = Autograd.Node(op="leaf", inputs=[], saved=[], shape=a.shape, value=a, axis=0, param=0.0, keep=False)
    return [
        Check.equal("autograd.empty", len(Autograd.empty().nodes), 0),
        Check.equal("autograd.Step", step.out, 1),
        Check.equal("autograd.Grad", contribution.pos, 0),
        vector("autograd.Node", node.value, [2.0, 4.0]),
        vector("autograd.value_of", Autograd.value_of(step.tape, step.out), [4.0, 16.0]),
        vector("autograd.backward", unwrap_or(listGet(grads, leaf.out), Array.empty()), [4.0, 8.0]),
    ]


def sub_div_neg(tape, x):
    divided = Autograd.div(tape, x, x)
    subtracted = Autograd.sub(divided.tape, divided.out, x)
    return Autograd.neg(subtracted.tape, subtracted.out)


def axis_mean_sum(tape, x):
    mean = Autograd.mean_axis(tape, x, 1, False)
    return Autograd.sum_axis(mean.tape, mean.out, 0, False)


def log_softmax_sum(tape, x):
    logged = Autograd.log_softmax(tape, x, 0)
    return Autograd.sum_all(logged.tape, logged.out)


def grad_cases():
    a = Array.array1([2.0, 4.0])
    pulled = Autograd.vjp(Autograd.square, a, Array.array1([3.0, 5.0]))
    explicit = Autograd.VJP(primal=pulled.primal, cotangent=pulled.cotangent)
    return [
        vector("autograd.sub_div_neg", Autograd.grad(sub_div_neg, a), [1.0, 1.0]),
        vector("autograd.mean_axis_sum_axis", Autograd.grad(axis_mean_sum, Array.array2([[1.0, 2.0], [3.0, 4.0]])), [0.5, 0.5, 0.5, 0.5]),
        vector("autograd.log_softmax", Autograd.grad(log_softmax_sum, Array.array1([0.0, 0.0])), [0.0, 0.0]),
        vector("autograd.vjp.primal", explicit.primal, [4.0, 16.0]),
        vector("autograd.vjp.cotangent", explicit.cotangent, [12.0, 40.0]),
    ]


def optimizer_records():
    a = Array.array1([2.0])
    sgd = Optim.SgdResult(params=[a], state=Optim.SgdState(buffers=[]))
    adam = Optim.AdamResult(params=[a], state=Optim.AdamState(exp_avg=[], exp_avg_sq=[], max_exp_avg_sq=[], step=2))
    rms = Optim.RmspropResult(params=[a], state=Optim.RmspropState(square_avg=[], grad_avg=[], buffers=[]))
    ada = Optim.AdagradResult(params=[a], state=Optim.AdagradState(sum=[], step=3))
    clip = Optim.ClipResult(grads=[a], total_norm=2.0)
    return [
        Check.equal("optim.SgdResult", len(sgd.params), 1),
        Check.equal("optim.AdamResult", adam.state.step, 2),
        Check.equal("optim.RmspropResult", len(rms.params), 1),
        Check.equal("optim.AdagradResult", ada.state.step, 3),
        Check.equal("optim.ClipResult", clip.total_norm, 2.0),
    ]


def unsupported_norm(order):
    grads = [Array.array1([3.0, 4.0])]
    result = Errors.checked_float(lambda: Optim.clip_grad_norm_(grads, 1.0, order).total_norm)
    name = f"optim.unsupported_norm.{order}"
    match result:
        case Error(message=message):
            return Check.equal(name, message, "only L2 and infinity gradient norms are implemented")
        case Success(value=value):
            return Check.failed(name, f"accepted unsupported norm order: {value}")


def checked_cases():
    with Errors.raising:
        groups = [tensor_binary_cases(), tensor_scalar_cases(), tensor_layout_cases(), tape_cases(), grad_cases(), optimizer_records(), mapList([-1.0, 0.0, 1.0], unsupported_norm)]
        return Success(value=foldList(groups, [], listConcat))


def run():
    match checked_cases():
        case Success(value=value):
            return value
        case Error(message=message):
            return [Check.failed("torch.contracts", message)]
