#!/bin/sh
# Implements [CI-PUSH-GREEN] and [CI-PUSH-ONE-RUN] (docs/plans/coordinated-push.md).
# Pushes the current branch only when `make ci` passed on this exact HEAD and
# no CI run on the branch is queued or running, so a push never cancels or
# races another agent's run. Hold the TMC lock PUSH-<branch> before calling it.
set -eu
stamp=target/ci-green
head=$(git rev-parse HEAD)
branch=$(git rev-parse --abbrev-ref HEAD)
if [ "$(cat "$stamp" 2>/dev/null)" != "$head" ]; then
    echo "push refused: make ci has not passed on $head (run make ci, then scripts/ci/push.sh)" >&2
    exit 1
fi
busy=$(gh run list --branch "$branch" --limit 20 --json status \
    --jq '[.[] | select(.status != "completed")] | length')
if [ "$busy" != "0" ]; then
    echo "push refused: $busy CI run(s) on $branch still queued or running; wait for them" >&2
    exit 1
fi
# The branch can advance while GitHub reports the run status.
git push origin "$head:refs/heads/$branch"
