# osprey: flavor=python
# osprey: namespace=torch
# osprey: module=Optim
"""torch.optim: SGD, Adam, AdamW, RMSprop and Adagrad over parameter lists.

Parameters and gradients are `numpy.Array.NDArray[B]` tensors, one per
parameter in `named_parameters()` order. A step is a pure function returning
the updated parameters and the updated state, because values are immutable and
in-place mutation is unspellable [PORT-SCOPE]. Each update follows torch's
float64 element order exactly — the same scalar products, divisions and
additions in the same sequence — so parameters match torch's after any number
of steps at rtol 1e-12.
"""
from numpy import Array, Errors
from torch import Tensor

__all__ = [
    "SgdState", "SgdResult", "AdamState", "AdamResult", "RmspropState",
    "RmspropResult", "AdagradState", "AdagradResult", "ClipResult", "sgd",
    "adam", "adamw", "rmsprop", "adagrad", "clip_grad_norm_", "clip_grad_value_",
]

class SgdState[B]:
    """SGD's momentum buffer per parameter, empty until the first step."""
    buffers: list[Array.NDArray[B]]

class SgdResult[B]:
    """An SGD step: the updated parameters and the new state."""
    params: list[Array.NDArray[B]]
    state: SgdState[B]

class AdamState[B]:
    """Adam's moments per parameter, the amsgrad maximum and the step count."""
    exp_avg: list[Array.NDArray[B]]
    exp_avg_sq: list[Array.NDArray[B]]
    max_exp_avg_sq: list[Array.NDArray[B]]
    step: int

class AdamResult[B]:
    """An Adam/AdamW step: the updated parameters and the new state."""
    params: list[Array.NDArray[B]]
    state: AdamState[B]

class RmspropState[B]:
    """RMSprop's square average, gradient average and momentum buffer."""
    square_avg: list[Array.NDArray[B]]
    grad_avg: list[Array.NDArray[B]]
    buffers: list[Array.NDArray[B]]

class RmspropResult[B]:
    """An RMSprop step: the updated parameters and the new state."""
    params: list[Array.NDArray[B]]
    state: RmspropState[B]

class AdagradState[B]:
    """Adagrad's running gradient-square sum, empty until the first step."""
    sum: list[Array.NDArray[B]]
    step: int

class AdagradResult[B]:
    """An Adagrad step: the updated parameters and the new state."""
    params: list[Array.NDArray[B]]
    state: AdagradState[B]

class ClipResult[B]:
    """clip_grad_norm_'s scaled gradients and the total norm torch returns."""
    grads: list[Array.NDArray[B]]
    total_norm: float

def param_at(values, i):
    """The `i`th tensor of a per-parameter list, or the empty tensor numpy's
    validated access needs."""
    return unwrap_or(listGet(values, i), Array.empty())

def decayed(g, p, weight_decay):
    """The gradient with torch's L2 weight decay added, `g += wd * p`."""
    return Array.add(g, Tensor.scale(p, weight_decay)) if weight_decay != 0.0 else g

class ParamGrad[B]:
    """One parameter paired with the gradient its update consumes."""
    param: Array.NDArray[B]
    grad: Array.NDArray[B]

def grad_at(params, grads, i, weight_decay):
    """The `i`th parameter and its gradient with torch's L2 weight decay."""
    p = param_at(params, i)
    return ParamGrad(param=p, grad=decayed(param_at(grads, i), p, weight_decay))

def moment(values, p, i):
    """One per-parameter optimizer buffer: zero before the first step, when the
    state list is still empty."""
    return Array.zeros_like(p) if len(values) == 0 else param_at(values, i)

def blend(prev, x, beta):
    """An exponential running average, `beta * prev + (1 - beta) * x`."""
    return Array.add(Tensor.scale(prev, beta), Tensor.scale(x, Tensor.one_minus(beta)))

def descend(p, d, lr):
    """One gradient-descent step, `p - lr * d`."""
    return Array.add(p, Tensor.scale(d, 0.0 - lr))

def append_if(cond, xs, x):
    """`xs + [x]` only when `cond` holds; torch keeps no unused buffers."""
    return listAppend(xs, x) if cond else xs

def sgd(params, grads, state, lr, momentum, dampening, nesterov, weight_decay):
    """torch.optim.SGD.step: L2 weight decay, then a momentum buffer
    `buf = momentum * buf + (1 - dampening) * grad`, Nesterov adding
    `momentum * buf` on top, and `p -= lr * d`."""
    acc = fold(range(len(params)), empty_work(), lambda a, i: sgd_param(a, i, params, grads, state, lr, momentum, dampening, nesterov, weight_decay))
    return SgdResult(params=acc.params, state=SgdState(buffers=acc.state))

class Work[B]:
    """A fold accumulator over the parameters, the `state` list (SGD's momentum
    buffers or Adagrad's square sums) and three further buffer lists — Adam's
    moments and amsgrad maximum, or RMSprop's square/gradient averages."""
    params: list[Array.NDArray[B]]
    state: list[Array.NDArray[B]]
    first: list[Array.NDArray[B]]
    second: list[Array.NDArray[B]]
    third: list[Array.NDArray[B]]

def empty_work():
    """A fold accumulator before the first parameter update."""
    return Work(params=[], state=[], first=[], second=[], third=[])

def kept(w, p, s):
    """`w` with its parameter and state lists replaced, the buffers kept."""
    return Work(params=p, state=s, first=w.first, second=w.second, third=w.third)

def buffered(w, p, first, second, third):
    """`w` with its parameter and three buffer lists replaced, state kept."""
    return Work(params=p, state=w.state, first=first, second=second, third=third)

def sgd_param(a, i, params, grads, state, lr, momentum, dampening, nesterov, weight_decay):
    """One SGD parameter update, in torch's element order. torch keeps no
    momentum buffer when momentum is 0."""
    pg = grad_at(params, grads, i, weight_decay)
    if momentum == 0.0:
        return kept(a, listAppend(a.params, descend(pg.param, pg.grad, lr)), a.state)
    else:
        buf = pg.grad if len(state.buffers) == 0 else Array.add(Tensor.scale(unwrap_or(listGet(state.buffers, i), pg.grad), momentum), Tensor.scale(pg.grad, Tensor.one_minus(dampening)))
        d3 = Array.add(pg.grad, Tensor.scale(buf, momentum)) if nesterov else buf
        return kept(a, listAppend(a.params, descend(pg.param, d3, lr)), listAppend(a.state, buf))

def adam(params, grads, state, lr, beta1, beta2, eps, weight_decay, amsgrad):
    """torch.optim.Adam.step: L2 weight decay, moment decay, bias correction."""
    return adam_step(params, grads, state, lr, beta1, beta2, eps, weight_decay, amsgrad, False)

def adam_step(params, grads, state, lr, beta1, beta2, eps, weight_decay, amsgrad, decoupled):
    """Shared moment and bias-correction update; decay placement stays explicit."""
    t = state.step + 1
    bc1 = Tensor.one_minus(Tensor.pow_f(beta1, t))
    bc2 = Tensor.one_minus(Tensor.pow_f(beta2, t))
    step_size = lr / bc1
    acc = fold(range(len(params)), empty_work(), lambda a, i: adam_param(a, i, params, grads, state, beta1, beta2, eps, weight_decay, amsgrad, step_size, bc2, lr, decoupled))
    return AdamResult(params=acc.params, state=AdamState(exp_avg=acc.first, exp_avg_sq=acc.second, max_exp_avg_sq=acc.third, step=t))

def adam_param(a, i, params, grads, state, beta1, beta2, eps, weight_decay, amsgrad, step_size, bc2, lr, decoupled):
    """One Adam/AdamW update, retaining each algorithm's decay placement."""
    pg = grad_at(params, grads, i, 0.0 if decoupled else weight_decay)
    g = pg.grad
    e0 = moment(state.exp_avg, pg.param, i)
    s0 = moment(state.exp_avg_sq, pg.param, i)
    m0 = moment(state.max_exp_avg_sq, pg.param, i)
    m = blend(e0, g, beta1)
    s = blend(s0, Array.multiply(g, g), beta2)
    mx = Array.maximum(m0, s) if amsgrad else s
    denom = Tensor.add_scalar(Tensor.div_scalar(Array.sqrt(mx), sqrt(bc2)), eps)
    base = Tensor.scale(pg.param, 1.0 - lr * weight_decay) if decoupled else pg.param
    p2 = descend(base, Array.divide(m, denom), step_size)
    return buffered(a, listAppend(a.params, p2), listAppend(a.first, m), listAppend(a.second, s), append_if(amsgrad, a.third, mx))

def adamw(params, grads, state, lr, beta1, beta2, eps, weight_decay, amsgrad):
    """torch.optim.AdamW.step: decoupled weight decay, then the Adam update."""
    return adam_step(params, grads, state, lr, beta1, beta2, eps, weight_decay, amsgrad, True)

def rmsprop(params, grads, state, lr, alpha, eps, weight_decay, momentum, centered):
    """torch.optim.RMSprop.step: running square average `alpha * sq + (1-alpha)
    * g^2`, optional centered variance, optional momentum, L2 weight decay."""
    acc = fold(range(len(params)), empty_work(), lambda a, i: rmsprop_param(a, i, params, grads, state, lr, alpha, eps, weight_decay, momentum, centered))
    return RmspropResult(params=acc.params, state=RmspropState(square_avg=acc.first, grad_avg=acc.second, buffers=acc.third))

def rmsprop_param(a, i, params, grads, state, lr, alpha, eps, weight_decay, momentum, centered):
    """One RMSprop parameter update, in torch's element order."""
    pg = grad_at(params, grads, i, weight_decay)
    sq0 = moment(state.square_avg, pg.param, i)
    sq = blend(sq0, Array.multiply(pg.grad, pg.grad), alpha)
    ga0 = moment(state.grad_avg, pg.param, i)
    ga = blend(ga0, pg.grad, alpha) if centered else ga0
    avg = Tensor.add_scalar(Array.sqrt(Array.subtract(sq, Array.multiply(ga, ga))), eps) if centered else Tensor.add_scalar(Array.sqrt(sq), eps)
    buf0 = moment(state.buffers, pg.param, i)
    buf = Array.add(Tensor.scale(buf0, momentum), Array.divide(pg.grad, avg)) if momentum > 0.0 else buf0
    d = buf if momentum > 0.0 else Array.divide(pg.grad, avg)
    return buffered(a, listAppend(a.params, descend(pg.param, d, lr)), listAppend(a.first, sq), append_if(centered, a.second, ga), append_if(momentum > 0.0, a.third, buf))

def adagrad(params, grads, state, lr, lr_decay, weight_decay, eps):
    """torch.optim.Adagrad.step: running gradient-square sum, and
    `p -= (lr / (1 + (step - 1) * lr_decay)) * (g / (sqrt(sum) + eps))`."""
    t = state.step + 1
    clr = lr / (1.0 + toFloat(t - 1) * lr_decay)
    acc = fold(range(len(params)), empty_work(), lambda a, i: adagrad_param(a, i, params, grads, state, clr, weight_decay, eps))
    return AdagradResult(params=acc.params, state=AdagradState(sum=acc.state, step=t))

def adagrad_param(a, i, params, grads, state, clr, weight_decay, eps):
    """One Adagrad parameter update, in torch's element order."""
    pg = grad_at(params, grads, i, weight_decay)
    s0 = moment(state.sum, pg.param, i)
    s = Array.add(s0, Array.multiply(pg.grad, pg.grad))
    std = Tensor.add_scalar(Array.sqrt(s), eps)
    p2 = descend(pg.param, Array.divide(pg.grad, std), clr)
    return kept(a, listAppend(a.params, p2), listAppend(a.state, s))

def clip_grad_norm_(grads, max_norm, norm_type):
    """torch.nn.utils.clip_grad_norm_: scale every gradient by
    `max_norm / (total_norm + 1e-6)` when that coefficient is below 1, and
    return the new gradients with torch's total norm. `norm_type` is 2.0 or
    +inf (torch's Lp and infinity cases)."""
    total = total_norm(grads, norm_type)
    coef = max_norm / (total + 0.000001)
    clipped = mapList(grads, lambda g: Tensor.scale(g, coef)) if coef < 1.0 else grads
    return ClipResult(grads=clipped, total_norm=total)

def total_norm(grads, norm_type):
    """The supported L2 and infinity gradient norms; reject other orders."""
    inf = exp(1000.0)
    _ = Errors.require(norm_type == 2.0 or norm_type == inf, "only L2 and infinity gradient norms are implemented")
    if norm_type == inf:
        return foldList(grads, 0.0 - inf, lambda acc, g: acc if acc >= max_abs(g) else max_abs(g))
    else:
        return sqrt(foldList(grads, 0.0, lambda acc, g: acc + param_norm_sq(g)))

def max_abs(g):
    """The largest absolute element of a gradient, torch's infinity norm."""
    return Array.max(Array.absolute(g))

def param_norm_sq(g):
    """The square of a parameter's L2 norm, `sqrt(sum(x^2))^2`, as torch's
    clip_grad_norm_ sums it."""
    n = sqrt(Array.sum(Array.multiply(g, g)))
    return n * n

def clip_grad_value_(grads, clip_value):
    """torch.nn.utils.clip_grad_value_: clamp every gradient element to
    `[-clip_value, clip_value]`, returning the new gradients."""
    return mapList(grads, lambda g: Array.clip(g, 0.0 - clip_value, clip_value))
