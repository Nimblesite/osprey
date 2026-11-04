# osprey: flavor=python
"""@osprey/torch's suite: the autograd tape, optimizers, learning-rate
schedulers and gradient clipping compared against the real torch (float64) via
scripts/oracles/torch.py, every case run under BOTH the host and the GPU numpy
storage handlers.

Floats compare with numpy's isclose rule: rtol 1e-12 for optimizers, schedulers
and clipping (only + - * / and sqrt), rtol 1e-9 for autograd gradients
downstream of exp/tanh/sigmoid/gelu (libm last-digit differences), and rtol 1e-6
for the gradcheck-style finite differences."""
from numpy import Array, Device, Errors, Host
from testing import Check
from torch import Autograd, LR, Optim
from torch_expected import Expected
from torch_tests import Contracts

TIGHT = 0.000000000001
LOOSE = 0.000000001
FD = 0.000001
ATOL = 0.0

SGD_SPECS = [[0.0, 0.0, 0.0, 0.0], [0.9, 0.0, 0.0, 0.0], [0.9, 0.0, 1.0, 0.0], [0.9, 0.3, 0.0, 0.01], [0.0, 0.0, 0.0, 0.01]]
ADAM_SPECS = [[0.01, 0.9, 0.999, 1e-08, 0.0, 0.0], [0.01, 0.9, 0.999, 1e-08, 0.01, 0.0], [0.01, 0.5, 0.9, 0.0001, 0.0, 1.0]]
ADAMW_SPECS = [[0.01, 0.9, 0.999, 1e-08, 0.01, 0.0], [0.01, 0.9, 0.999, 1e-08, 0.1, 1.0]]
RMS_SPECS = [[0.99, 1e-08, 0.0, 0.0, 0.0], [0.99, 1e-08, 0.0, 0.9, 1.0], [0.9, 1e-06, 0.01, 0.0, 1.0]]
ADA_SPECS = [[0.01, 0.0, 0.0, 1e-10], [0.1, 0.1, 0.01, 1e-10]]


def atl(xs, i):
    """The list at i, [] past the end."""
    return unwrap_or(listGet(xs, i), [])


def atf(xs, i):
    """The float at i, 0.0 past the end."""
    return unwrap_or(listGet(xs, i), 0.0)


def flat_expected(xs):
    """Flatten an oracle `[p0, p1]` pair into one float list."""
    return listConcat(atl(xs, 0), atl(xs, 1))


def flat_list(arrays):
    """Flatten a list of tensors into one float list."""
    return foldList(mapList(arrays, lambda a: Array.tolist(a)), [], lambda acc, xs: listConcat(acc, xs))


def apply(backend, work):
    """Run `work` (which wraps its compute in numpy's checked handler) under
    the host or GPU storage handler."""
    return Host.on_host(work) if backend == "host" else Device.on_gpu(work)


def check_list(name, result, want, rtol):
    """Compare a `Result[list[float], str]` against a flat float list."""
    match result:
        case Success(value=value):
            return Check.close_all(name, value, want, rtol, ATOL)
        case Error(message=message):
            return Check.failed(name, f"error: {message}")


def check_scalar(name, result, want, rtol):
    """Compare a `Result[float, str]` against a float."""
    match result:
        case Success(value=value):
            return Check.close(name, value, want, rtol, ATOL)
        case Error(message=message):
            return Check.failed(name, f"error: {message}")


def both(make):
    """A case's outcomes under both handlers."""
    return listConcat(make("host"), make("gpu"))


def arr(vals):
    return Array.array1(vals)


def arr2(rows):
    return Array.array2(rows)


def params0():
    return [arr([1.0, -2.0, 0.5, 3.0]), arr([-0.5, 1.5, 2.5])]


def grad_step(i):
    match i:
        case 0:
            return [arr([0.1, 0.2, -0.3, 0.4]), arr([0.05, -0.15, 0.25])]
        case 1:
            return [arr([-0.2, 0.3, 0.1, -0.4]), arr([0.35, 0.15, -0.05])]
        case _:
            return [arr([0.3, -0.1, 0.2, 0.5]), arr([-0.25, 0.05, 0.45])]


# Optimizer runners (three steps over the fixed gradient schedule) ---------

def sgd_3(momentum, dampening, nesterov, wd):
    r1 = Optim.sgd(params0(), grad_step(0), Optim.SgdState(buffers=[]), 0.1, momentum, dampening, nesterov, wd)
    r2 = Optim.sgd(r1.params, grad_step(1), r1.state, 0.1, momentum, dampening, nesterov, wd)
    return Optim.sgd(r2.params, grad_step(2), r2.state, 0.1, momentum, dampening, nesterov, wd)


def adam_3(lr, beta1, beta2, eps, wd, amsgrad):
    s = Optim.AdamState(exp_avg=[], exp_avg_sq=[], max_exp_avg_sq=[], step=0)
    r1 = Optim.adam(params0(), grad_step(0), s, lr, beta1, beta2, eps, wd, amsgrad)
    r2 = Optim.adam(r1.params, grad_step(1), r1.state, lr, beta1, beta2, eps, wd, amsgrad)
    return Optim.adam(r2.params, grad_step(2), r2.state, lr, beta1, beta2, eps, wd, amsgrad)


def adamw_3(lr, beta1, beta2, eps, wd, amsgrad):
    s = Optim.AdamState(exp_avg=[], exp_avg_sq=[], max_exp_avg_sq=[], step=0)
    r1 = Optim.adamw(params0(), grad_step(0), s, lr, beta1, beta2, eps, wd, amsgrad)
    r2 = Optim.adamw(r1.params, grad_step(1), r1.state, lr, beta1, beta2, eps, wd, amsgrad)
    return Optim.adamw(r2.params, grad_step(2), r2.state, lr, beta1, beta2, eps, wd, amsgrad)


def rmsprop_3(alpha, eps, wd, momentum, centered):
    s = Optim.RmspropState(square_avg=[], grad_avg=[], buffers=[])
    r1 = Optim.rmsprop(params0(), grad_step(0), s, 0.01, alpha, eps, wd, momentum, centered)
    r2 = Optim.rmsprop(r1.params, grad_step(1), r1.state, 0.01, alpha, eps, wd, momentum, centered)
    return Optim.rmsprop(r2.params, grad_step(2), r2.state, 0.01, alpha, eps, wd, momentum, centered)


def adagrad_3(lr, lr_decay, wd, eps):
    s = Optim.AdagradState(sum=[], step=0)
    r1 = Optim.adagrad(params0(), grad_step(0), s, lr, lr_decay, wd, eps)
    r2 = Optim.adagrad(r1.params, grad_step(1), r1.state, lr, lr_decay, wd, eps)
    return Optim.adagrad(r2.params, grad_step(2), r2.state, lr, lr_decay, wd, eps)


# Optimizer cases ---------------------------------------------------------

def checked_rows(work):
    with Errors.raising:
        return Success(value=work())


def optimizer_field(name, backend, i, j, value, want, suffixes):
    suffix = unwrap_or(listGet(suffixes, j), "missing")
    return Check.close_all(f"{name}.{suffix}.{i}.{backend}", atl(value, j), flat_expected(atl(want, j)), TIGHT, ATOL)


def check_optimizer(name, backend, i, work, want, suffixes):
    match apply(backend, lambda: checked_rows(work)):
        case Success(value=value):
            fields = mapList(toList(range(len(suffixes))), lambda j: optimizer_field(name, backend, i, j, value, want, suffixes))
            return listAppend(fields, Check.equal(f"{name}.fields.{i}.{backend}", len(value), len(suffixes)))
        case Error(message=message):
            return [Check.failed(f"{name}.{i}.{backend}", message)]


def sgd_rows(i):
    s = atl(SGD_SPECS, i)
    r = sgd_3(atf(s, 0), atf(s, 1), atf(s, 2) != 0.0, atf(s, 3))
    return [flat_list(r.params), flat_list(r.state.buffers)]


def sgd_case(b, i):
    want = [atl(Expected.SGD_PARAMS, i), atl(Expected.SGD_BUFFERS, i)]
    return check_optimizer("sgd", b, i, lambda: sgd_rows(i), want, ["params", "buffers"])


def sgd_cases():
    return fold(range(5), [], lambda acc, i: listConcat(acc, both(lambda b: sgd_case(b, i))))


def adam_rows(i, decoupled):
    s = atl(ADAMW_SPECS if decoupled else ADAM_SPECS, i)
    r = adamw_3(atf(s, 0), atf(s, 1), atf(s, 2), atf(s, 3), atf(s, 4), atf(s, 5) != 0.0) if decoupled else adam_3(atf(s, 0), atf(s, 1), atf(s, 2), atf(s, 3), atf(s, 4), atf(s, 5) != 0.0)
    return [flat_list(r.params), flat_list(r.state.exp_avg), flat_list(r.state.exp_avg_sq), flat_list(r.state.max_exp_avg_sq)]


def adam_case(b, i, decoupled):
    tables = [Expected.ADAMW_PARAMS, Expected.ADAMW_EXP_AVG, Expected.ADAMW_EXP_AVG_SQ, Expected.ADAMW_MAX] if decoupled else [Expected.ADAM_PARAMS, Expected.ADAM_EXP_AVG, Expected.ADAM_EXP_AVG_SQ, Expected.ADAM_MAX]
    want = mapList(tables, lambda xs: atl(xs, i))
    return check_optimizer("adamw" if decoupled else "adam", b, i, lambda: adam_rows(i, decoupled), want, ["params", "exp_avg", "exp_avg_sq", "max"])


def adam_cases():
    return fold(range(3), [], lambda acc, i: listConcat(acc, both(lambda b: adam_case(b, i, False))))


def adamw_cases():
    return fold(range(2), [], lambda acc, i: listConcat(acc, both(lambda b: adam_case(b, i, True))))


def rmsprop_rows(i):
    s = atl(RMS_SPECS, i)
    r = rmsprop_3(atf(s, 0), atf(s, 1), atf(s, 2), atf(s, 3), atf(s, 4) != 0.0)
    return [flat_list(r.params), flat_list(r.state.square_avg), flat_list(r.state.grad_avg), flat_list(r.state.buffers)]


def rmsprop_case(b, i):
    want = mapList([Expected.RMSPROP_PARAMS, Expected.RMSPROP_SQ, Expected.RMSPROP_GRAD_AVG, Expected.RMSPROP_BUFFERS], lambda xs: atl(xs, i))
    return check_optimizer("rmsprop", b, i, lambda: rmsprop_rows(i), want, ["params", "sq", "grad_avg", "buffers"])


def rmsprop_cases():
    return fold(range(3), [], lambda acc, i: listConcat(acc, both(lambda b: rmsprop_case(b, i))))


def adagrad_rows(i):
    s = atl(ADA_SPECS, i)
    r = adagrad_3(atf(s, 0), atf(s, 1), atf(s, 2), atf(s, 3))
    return [flat_list(r.params), flat_list(r.state.sum)]


def adagrad_case(b, i):
    want = [atl(Expected.ADAGRAD_PARAMS, i), atl(Expected.ADAGRAD_SUM, i)]
    return check_optimizer("adagrad", b, i, lambda: adagrad_rows(i), want, ["params", "sum"])


def adagrad_cases():
    return fold(range(2), [], lambda acc, i: listConcat(acc, both(lambda b: adagrad_case(b, i))))


# Learning-rate schedulers ------------------------------------------------

def step_lr_case(backend, e):
    compute = lambda: LR.step_lr(0.5, e, 3, 0.7)
    work = lambda: Errors.checked_float(compute)
    return check_scalar(f"lr.step.{e}.{backend}", apply(backend, work), atf(Expected.STEP_LRS, e), TIGHT)


def multi_lr_case(backend, e):
    compute = lambda: LR.multi_step_lr(0.5, e, [2, 5, 7], 0.5)
    work = lambda: Errors.checked_float(compute)
    return check_scalar(f"lr.multi.{e}.{backend}", apply(backend, work), atf(Expected.MULTI_LRS, e), TIGHT)


def expo_lr_case(backend, e):
    compute = lambda: LR.exponential_lr(0.5, e, 0.8)
    work = lambda: Errors.checked_float(compute)
    return check_scalar(f"lr.exponential.{e}.{backend}", apply(backend, work), atf(Expected.EXPONENTIAL_LRS, e), TIGHT)


def lambda_lr_case(backend, e):
    compute = lambda: LR.lambda_lr(0.5, e, lambda t: 1.0 / (1.0 + 0.1 * toFloat(t)))
    work = lambda: Errors.checked_float(compute)
    return check_scalar(f"lr.lambda.{e}.{backend}", apply(backend, work), atf(Expected.LAMBDA_LRS, e), TIGHT)


def linear_lr_case(backend, e):
    compute = lambda: LR.linear_lr(0.5, e, 1.0 / 3.0, 1.0, 5)
    work = lambda: Errors.checked_float(compute)
    return check_scalar(f"lr.linear.{e}.{backend}", apply(backend, work), atf(Expected.LINEAR_LRS, e), TIGHT)


def lr_cases():
    step = fold(range(9), [], lambda acc, e: listConcat(acc, both(lambda b: [step_lr_case(b, e)])))
    multi = fold(range(9), [], lambda acc, e: listConcat(acc, both(lambda b: [multi_lr_case(b, e)])))
    expo = fold(range(9), [], lambda acc, e: listConcat(acc, both(lambda b: [expo_lr_case(b, e)])))
    lam = fold(range(9), [], lambda acc, e: listConcat(acc, both(lambda b: [lambda_lr_case(b, e)])))
    lin = fold(range(9), [], lambda acc, e: listConcat(acc, both(lambda b: [linear_lr_case(b, e)])))
    return listConcat(listConcat(listConcat(step, multi), listConcat(expo, lam)), lin)


# Gradient clipping -------------------------------------------------------

def clip_grads():
    return [arr([3.0, 4.0, 0.0, -5.0]), arr([0.6, -0.8])]


def clip_norm_grads(backend, i):
    norm_type = 2.0 if i < 2 else exp(1000.0)
    max_norm = 1.0 if i != 1 else 10.0
    compute = lambda: flat_list(Optim.clip_grad_norm_(clip_grads(), max_norm, norm_type).grads)
    work = lambda: Errors.checked_values(compute)
    return check_list(f"clip.norm.grads.{i}.{backend}", apply(backend, work), flat_expected(atl(Expected.CLIP_NORM_GRADS, i)), TIGHT)


def clip_norm_total(backend, i):
    norm_type = 2.0 if i < 2 else exp(1000.0)
    max_norm = 1.0 if i != 1 else 10.0
    compute = lambda: Optim.clip_grad_norm_(clip_grads(), max_norm, norm_type).total_norm
    work = lambda: Errors.checked_float(compute)
    return check_scalar(f"clip.norm.total.{i}.{backend}", apply(backend, work), atf(Expected.CLIP_NORM_TOTAL, i), TIGHT)


def clip_value_case(backend):
    compute = lambda: flat_list(Optim.clip_grad_value_(clip_grads(), 0.5))
    work = lambda: Errors.checked_values(compute)
    return check_list(f"clip.value.{backend}", apply(backend, work), flat_expected(Expected.CLIP_VALUE_GRADS), TIGHT)


def clip_cases():
    n2 = fold(range(3), [], lambda acc, i: listConcat(acc, both(lambda b: [clip_norm_grads(b, i), clip_norm_total(b, i)])))
    return listConcat(n2, both(lambda b: [clip_value_case(b)]))


# Autograd ----------------------------------------------------------------

def f_expmul(tape, x):
    e = Autograd.exp(tape, x)
    m = Autograd.mul(e.tape, x, e.out)
    return Autograd.sum_all(m.tape, m.out)


def f_tanh_sig(tape, x):
    t = Autograd.tanh(tape, x)
    s = Autograd.sigmoid(t.tape, x)
    a = Autograd.add(s.tape, t.out, s.out)
    return Autograd.sum_all(a.tape, a.out)


def f_relu_lrelu(tape, x):
    r = Autograd.relu(tape, x)
    l = Autograd.leaky_relu(r.tape, x, 0.1)
    a = Autograd.add(l.tape, r.out, l.out)
    return Autograd.sum_all(a.tape, a.out)


def f_gelu_silu(tape, x):
    g = Autograd.gelu(tape, x)
    s = Autograd.silu(g.tape, x)
    a = Autograd.add(s.tape, g.out, s.out)
    return Autograd.sum_all(a.tape, a.out)


def f_square_mean(tape, x):
    s = Autograd.square(tape, x)
    return Autograd.mean_all(s.tape, s.out)


def f_log_sqrt(tape, x):
    l = Autograd.log(tape, x)
    s = Autograd.sqrt(l.tape, x)
    a = Autograd.add(s.tape, l.out, s.out)
    return Autograd.sum_all(a.tape, a.out)


def f_softmax(tape, x):
    s = Autograd.softmax(tape, x, 0)
    m = Autograd.mul(s.tape, x, s.out)
    return Autograd.sum_all(m.tape, m.out)


def f_matmul(tape, x):
    w = Autograd.leaf(tape, arr2([[0.2, -0.5], [0.7, 0.3], [-0.4, 0.6]]))
    mm = Autograd.matmul(w.tape, x, w.out)
    return Autograd.sum_all(mm.tape, mm.out)


def f_broadcast(tape, x):
    y = Autograd.leaf(tape, arr2([[1.0], [2.0]]))
    m = Autograd.mul(y.tape, x, y.out)
    return Autograd.sum_all(m.tape, m.out)


def f_bcast_reduce(tape, x):
    y = Autograd.leaf(tape, arr2([[1.0, 2.0, 3.0], [-1.0, 0.5, 2.0]]))
    m = Autograd.mul(y.tape, x, y.out)
    return Autograd.sum_all(m.tape, m.out)


def f_bcast_leading(tape, x):
    y = Autograd.leaf(tape, arr2([[1.0, 2.0, 3.0], [-1.0, 0.5, 2.0]]))
    m = Autograd.mul(y.tape, x, y.out)
    return Autograd.sum_all(m.tape, m.out)


def x1():
    return arr([0.3, -0.7, 1.1, -0.2, 0.9])


def xp():
    return arr([0.4, 1.2, 2.5])


def x2():
    return arr2([[0.5, -0.3, 1.0], [-1.2, 0.8, 0.4]])


def xb():
    return arr([0.5, -0.4, 1.2])


def xl():
    return arr2([[1.0], [2.0]])


def grad_of(f, make_x):
    return Array.tolist(Autograd.grad(f, make_x()))


def auto_compute(i):
    match i:
        case 0:
            return lambda: grad_of(f_expmul, x1)
        case 1:
            return lambda: grad_of(f_tanh_sig, x1)
        case 2:
            return lambda: grad_of(f_relu_lrelu, x1)
        case 3:
            return lambda: grad_of(f_gelu_silu, x1)
        case 4:
            return lambda: grad_of(f_square_mean, x1)
        case 5:
            return lambda: grad_of(f_log_sqrt, xp)
        case 6:
            return lambda: grad_of(f_softmax, x1)
        case 7:
            return lambda: grad_of(f_matmul, x2)
        case 8:
            return lambda: grad_of(f_broadcast, x2)
        case 9:
            return lambda: grad_of(f_bcast_reduce, xb)
        case _:
            return lambda: grad_of(f_bcast_leading, xl)


def auto_case(backend, i):
    compute = auto_compute(i)
    work = lambda: Errors.checked_values(compute)
    result = apply(backend, work)
    return [
        check_list(f"auto.analytic.{i}.{backend}", result, atl(Expected.AUTO_ANALYTIC, i), LOOSE),
        check_list(f"auto.fd.{i}.{backend}", result, atl(Expected.AUTO_FD, i), FD),
    ]


def auto_cases():
    return fold(range(11), [], lambda acc, i: listConcat(acc, both(lambda b: auto_case(b, i))))


# Suite -------------------------------------------------------------------

def suite():
    return foldList([sgd_cases(), adam_cases(), adamw_cases(), rmsprop_cases(), adagrad_cases(), lr_cases(), clip_cases(), auto_cases(), both(lambda b: apply(b, Contracts.run))], [], lambda acc, cases: listConcat(acc, cases))


def main():
    match Check.strictly(lambda: Check.report(suite())):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
