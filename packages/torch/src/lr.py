# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=torch
# osprey: module=LR
"""torch.optim.lr_scheduler: StepLR, MultiStepLR, ExponentialLR, LambdaLR and
LinearLR as pure functions.

torch's schedulers are stateful objects wrapping an optimizer; the port spells
each as a function of the step count (`last_epoch`) and the scheduler's
hyperparameters, returning the learning rate torch's `get_lr()` would produce
at that step. The step count is the number of scheduler steps taken, starting
at 0, exactly as torch increments `last_epoch` on each `step()`.
"""
from torch import Tensor

__all__ = [
    "step_lr", "multi_step_lr", "exponential_lr", "lambda_lr", "linear_lr",
]


def step_lr(base_lr, step, step_size, gamma):
    """StepLR: `base_lr * gamma ** (step // step_size)`."""
    return base_lr * Tensor.pow_f(gamma, intDiv(step, step_size))


def multi_step_lr(base_lr, step, milestones, gamma):
    """MultiStepLR: `base_lr * gamma ** k` where `k` is the count of
    milestones at or below `step` (torch's `bisect_right`)."""
    k = foldList(milestones, 0, lambda n, m: n + 1 if m <= step else n)
    return base_lr * Tensor.pow_f(gamma, k)


def exponential_lr(base_lr, step, gamma):
    """ExponentialLR: `base_lr * gamma ** step`."""
    return base_lr * Tensor.pow_f(gamma, step)


def lambda_lr(base_lr, step, lr_lambda):
    """LambdaLR: `base_lr * lr_lambda(step)` with the caller's factor
    function."""
    return base_lr * lr_lambda(step)


def linear_lr(initial_lr, step, start_factor, end_factor, total_iters):
    """LinearLR: `initial_lr * start_factor` at step 0, then each step up to
    `total_iters` multiplies by torch's factor
    `1 + (end - start) / (total * start + (step - 1) * (end - start))`,
    plateauing at `initial_lr * end_factor` past `total_iters`."""
    n = step if step < total_iters else total_iters
    d = end_factor - start_factor
    return fold(range(1, n + 1), initial_lr * start_factor, lambda acc, k: acc * (1.0 + d / (toFloat(total_iters) * start_factor + toFloat(k - 1) * d)))
