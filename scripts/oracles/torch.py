"""CPython oracle for @osprey/torch: optimizers, learning-rate schedulers,
gradient clipping and the autograd tape, all against real torch in float64.

Writes packages/torch/tests/expected.py, which the suite compares the port
against ([PORT-TEST-ORACLE], spec 0043). Run with the oracle virtualenv:
`python -I scripts/oracles/oracle.py torch`. Every value here is computed by torch (or
by finite differences of torch's forward), never hand-typed.
"""
from expected_module import write

import torch
import torch.nn.functional as F

torch.set_default_dtype(torch.float64)

# Initial parameters (two 1-D tensors, as named_parameters() would list them)
# and a fixed gradient schedule.
P0 = torch.tensor([1.0, -2.0, 0.5, 3.0])
P1 = torch.tensor([-0.5, 1.5, 2.5])
G0 = [torch.tensor([0.1, 0.2, -0.3, 0.4]), torch.tensor([0.05, -0.15, 0.25])]
G1 = [torch.tensor([-0.2, 0.3, 0.1, -0.4]), torch.tensor([0.35, 0.15, -0.05])]
G2 = [torch.tensor([0.3, -0.1, 0.2, 0.5]), torch.tensor([-0.25, 0.05, 0.45])]
GRADS = [G0, G1, G2]


def clone_params():
    return [P0.clone(), P1.clone()]


def flatten(params):
    return [t.reshape(-1).tolist() for t in params]


def run_optimizer(opt, steps=3):
    """Run `opt` for `steps` steps with the fixed gradient schedule, returning
    the final parameters and the final per-tensor state flattened."""
    for i in range(steps):
        for p, g in zip(opt.param_groups[0]["params"], GRADS[i]):
            p.grad = g
        opt.step()
    params = [p.detach() for p in opt.param_groups[0]["params"]]
    state = {}
    for p in opt.param_groups[0]["params"]:
        st = opt.state[p]
        for key, val in st.items():
            if isinstance(val, torch.Tensor):
                state.setdefault(key, []).append(val.detach().reshape(-1).tolist())
            else:
                state.setdefault(key, []).append(val)
    return flatten(params), state


# --- Optimizers -----------------------------------------------------------

def sgd_cases():
    rows = []
    for momentum, dampening, nesterov, wd in [
        (0.0, 0.0, False, 0.0),
        (0.9, 0.0, False, 0.0),
        (0.9, 0.0, True, 0.0),
        (0.9, 0.3, False, 0.01),
        (0.0, 0.0, False, 0.01),
    ]:
        params = clone_params()
        opt = torch.optim.SGD(params, lr=0.1, momentum=momentum, dampening=dampening, nesterov=nesterov, weight_decay=wd)
        p, s = run_optimizer(opt)
        rows.append({"params": p, "momentum_buffer": s.get("momentum_buffer", [[], []])})
    return rows


def adam_cases():
    rows = []
    for betas, eps, wd, amsgrad in [
        ((0.9, 0.999), 1e-8, 0.0, False),
        ((0.9, 0.999), 1e-8, 0.01, False),
        ((0.5, 0.9), 1e-4, 0.0, True),
    ]:
        params = clone_params()
        opt = torch.optim.Adam(params, lr=0.01, betas=betas, eps=eps, weight_decay=wd, amsgrad=amsgrad)
        p, s = run_optimizer(opt)
        rows.append({"params": p, "exp_avg": s.get("exp_avg", [[], []]), "exp_avg_sq": s.get("exp_avg_sq", [[], []]), "max_exp_avg_sq": s.get("max_exp_avg_sq", [[], []])})
    return rows


def adamw_cases():
    rows = []
    for betas, eps, wd, amsgrad in [
        ((0.9, 0.999), 1e-8, 0.01, False),
        ((0.9, 0.999), 1e-8, 0.1, True),
    ]:
        params = clone_params()
        opt = torch.optim.AdamW(params, lr=0.01, betas=betas, eps=eps, weight_decay=wd, amsgrad=amsgrad)
        p, s = run_optimizer(opt)
        rows.append({"params": p, "exp_avg": s.get("exp_avg", [[], []]), "exp_avg_sq": s.get("exp_avg_sq", [[], []]), "max_exp_avg_sq": s.get("max_exp_avg_sq", [[], []])})
    return rows


def rmsprop_cases():
    rows = []
    for alpha, eps, wd, momentum, centered in [
        (0.99, 1e-8, 0.0, 0.0, False),
        (0.99, 1e-8, 0.0, 0.9, True),
        (0.9, 1e-6, 0.01, 0.0, True),
    ]:
        params = clone_params()
        opt = torch.optim.RMSprop(params, lr=0.01, alpha=alpha, eps=eps, weight_decay=wd, momentum=momentum, centered=centered)
        p, s = run_optimizer(opt)
        rows.append({"params": p, "square_avg": s.get("square_avg", [[], []]), "grad_avg": s.get("grad_avg", [[], []]), "momentum_buffer": s.get("momentum_buffer", [[], []])})
    return rows


def adagrad_cases():
    rows = []
    for lr, lr_decay, wd, eps in [
        (0.01, 0.0, 0.0, 1e-10),
        (0.1, 0.1, 0.01, 1e-10),
    ]:
        params = clone_params()
        opt = torch.optim.Adagrad(params, lr=lr, lr_decay=lr_decay, weight_decay=wd, eps=eps)
        p, s = run_optimizer(opt)
        rows.append({"params": p, "sum": s.get("sum", [[], []])})
    return rows


# --- Learning-rate schedulers --------------------------------------------

def scheduler_lrs():
    """lr at each last_epoch 0..8 for each scheduler, torch's get_last_lr()."""
    out = {}
    out["step"] = _epoch_lrs(lambda o: torch.optim.lr_scheduler.StepLR(o, step_size=3, gamma=0.7))
    out["multi"] = _epoch_lrs(lambda o: torch.optim.lr_scheduler.MultiStepLR(o, milestones=[2, 5, 7], gamma=0.5))
    out["exponential"] = _epoch_lrs(lambda o: torch.optim.lr_scheduler.ExponentialLR(o, gamma=0.8))
    out["lambda"] = _epoch_lrs(lambda o: torch.optim.lr_scheduler.LambdaLR(o, lr_lambda=lambda e: 1.0 / (1.0 + 0.1 * e)))
    out["linear"] = _epoch_lrs(lambda o: torch.optim.lr_scheduler.LinearLR(o, start_factor=1.0 / 3.0, end_factor=1.0, total_iters=5))
    return out


def _epoch_lrs(sched_factory):
    """The lr at last_epoch 0..8: build a fresh scheduler for each epoch and
    step torch's machinery until its last_epoch equals the target, then read
    get_last_lr() (this respects LinearLR's _initial_step)."""
    base = 0.5
    out = []
    for e in range(9):
        p = torch.tensor([1.0])
        opt = torch.optim.SGD([p], lr=base)
        p.grad = torch.tensor([0.1])
        sched = sched_factory(opt)
        for _ in range(e - sched.last_epoch):
            opt.step()
            sched.step()
        out.append(sched.get_last_lr()[0])
    return out


# --- Gradient clipping ----------------------------------------------------

def clip_cases():
    grads = [torch.tensor([3.0, 4.0, 0.0, -5.0]), torch.tensor([0.6, -0.8])]
    rows = []
    for max_norm, norm_type in [(1.0, 2.0), (10.0, 2.0), (1.0, float("inf"))]:
        params = [torch.zeros_like(g) for g in grads]
        for p, g in zip(params, grads):
            p.grad = g.clone()
        total = torch.nn.utils.clip_grad_norm_(params, max_norm=max_norm, norm_type=norm_type)
        rows.append({"grads": [p.grad.tolist() for p in params], "total_norm": total.item()})
    params = [torch.zeros_like(g) for g in grads]
    for p, g in zip(params, grads):
        p.grad = g.clone()
    torch.nn.utils.clip_grad_value_(params, clip_value=0.5)
    rows.append({"value_grads": [p.grad.tolist() for p in params]})
    return rows


# --- Autograd -------------------------------------------------------------

X1 = torch.tensor([0.3, -0.7, 1.1, -0.2, 0.9])
XP = torch.tensor([0.4, 1.2, 2.5])  # positive, for log/sqrt
X2 = torch.tensor([[0.5, -0.3, 1.0], [-1.2, 0.8, 0.4]])
W2 = torch.tensor([[0.2, -0.5], [0.7, 0.3], [-0.4, 0.6]])
XB = torch.tensor([0.5, -0.4, 1.2])  # [3], broadcast against [2, 3]
YB = torch.tensor([[1.0, 2.0, 3.0], [-1.0, 0.5, 2.0]])
XL = torch.tensor([[1.0], [2.0]])  # [2, 1], broadcast against [2, 3]


def f_expmul(x):
    return (x * torch.exp(x)).sum()


def f_tanh_sig(x):
    return (torch.tanh(x) + torch.sigmoid(x)).sum()


def f_relu_lrelu(x):
    return (torch.relu(x) + F.leaky_relu(x, 0.1)).sum()


def f_gelu_silu(x):
    return (F.gelu(x) + F.silu(x)).sum()


def f_square_mean(x):
    return (x * x).mean()


def f_log_sqrt(x):
    return (torch.log(x) + torch.sqrt(x)).sum()


def f_softmax(x):
    return (torch.softmax(x, 0) * x).sum()


def f_matmul(x):
    return (x @ W2).sum()


def f_broadcast(x):
    y = torch.tensor([[1.0], [2.0]])
    return (x * y).sum()


def f_bcast_reduce(x):
    return (x * YB).sum()


def f_bcast_leading(x):
    return (x * YB).sum()


def analytic_grad(f, x):
    x = x.clone().requires_grad_(True)
    y = f(x)
    (g,) = torch.autograd.grad(y, x)
    return g.reshape(-1).tolist()


def fd_grad(f, x, h=2.0**-20):
    # Dyadic increments are exact at these input magnitudes. A decimal step
    # introduced cancellation noise even for a linear zero derivative.
    x = x.detach().clone()
    g = torch.zeros_like(x)
    n = x.numel()
    flat = x.reshape(-1)
    for i in range(n):
        orig = flat[i].item()
        flat[i] = orig + h
        fp = f(x).item()
        flat[i] = orig - h
        fm = f(x).item()
        flat[i] = orig
        g.reshape(-1)[i] = (fp - fm) / (2 * h)
    return g.reshape(-1).tolist()


def autograd_cases():
    rows = []
    for name, f, x in [
        ("expmul", f_expmul, X1),
        ("tanh_sig", f_tanh_sig, X1),
        ("relu_lrelu", f_relu_lrelu, X1),
        ("gelu_silu", f_gelu_silu, X1),
        ("square_mean", f_square_mean, X1),
        ("log_sqrt", f_log_sqrt, XP),
        ("softmax", f_softmax, X1),
        ("matmul", f_matmul, X2),
        ("broadcast", f_broadcast, X2),
        ("bcast_reduce", f_bcast_reduce, XB),
        ("bcast_leading", f_bcast_leading, XL),
    ]:
        rows.append({"name": name, "analytic": analytic_grad(f, x), "fd": fd_grad(f, x)})
    return rows


def case_values(prefix, cases, fields):
    return {f"{prefix}_{suffix}": [case[field] for case in cases] for suffix, field in fields.items()}


def optimizer_constants():
    moments = {"PARAMS": "params", "EXP_AVG": "exp_avg", "EXP_AVG_SQ": "exp_avg_sq", "MAX": "max_exp_avg_sq"}
    return {
        **case_values("SGD", sgd_cases(), {"PARAMS": "params", "BUFFERS": "momentum_buffer"}),
        **case_values("ADAM", adam_cases(), moments),
        **case_values("ADAMW", adamw_cases(), moments),
        **case_values("RMSPROP", rmsprop_cases(), {"PARAMS": "params", "SQ": "square_avg", "GRAD_AVG": "grad_avg", "BUFFERS": "momentum_buffer"}),
        **case_values("ADAGRAD", adagrad_cases(), {"PARAMS": "params", "SUM": "sum"}),
    }


def scheduler_constants():
    lrs = scheduler_lrs()
    return {f"{name.upper()}_LRS": values for name, values in lrs.items()}


def clipping_constants():
    clip = clip_cases()
    return {
        "CLIP_NORM_GRADS": [c["grads"] for c in clip[:3]],
        "CLIP_NORM_TOTAL": [c["total_norm"] for c in clip[:3]],
        "CLIP_VALUE_GRADS": clip[3]["value_grads"],
    }


def autograd_constants():
    auto = autograd_cases()
    return {"AUTO_ANALYTIC": [c["analytic"] for c in auto], "AUTO_FD": [c["fd"] for c in auto]}


def main():
    constants = {**optimizer_constants(), **scheduler_constants(), **clipping_constants(), **autograd_constants()}
    write("torch", "torch optimizers, LR schedulers, gradient clipping and autograd gradients (float64).", {"torch": torch.__version__}, constants)


if __name__ == "__main__":
    main()
