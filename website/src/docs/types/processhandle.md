---
layout: page
mlTwins: manual
title: "Process handle (int)"
description: "An integer process ID returned by spawnProcess, used to wait for completion and release runtime resources."
---

`spawnProcess` returns `Result<int, Error>`: the success value is the process ID used by `awaitProcess` and `cleanupProcess`. There is no separate built-in `ProcessHandle` type. The required callback receives `(processID, eventType, data)`; event types are stdout (1), stderr (2) and exit (3).

## Example

```osprey
fn onEvent(_pid, kind, data) = print("${kind}: ${trim(data)}")
match spawnProcess("echo hello", onEvent) {
    Success { value } => {
        let exitCode = awaitProcess(value)
        cleanupProcess(value)
        print("exit: ${exitCode}")
    }
    Error { message } => print("Process failed: ${message}")
}
```

```osprey-ml
onEvent (_pid, kind, data) = print "${kind}: ${trim data}"
match spawnProcess "echo hello" onEvent
    Success value =>
        exitCode = awaitProcess value
        cleanupProcess value
        print "exit: ${exitCode}"
    Error message => print "Process failed: ${message}"
```

The callback prints output and exit events; the final line reports the status returned by `awaitProcess`. Call `cleanupProcess` after waiting. The ML callback uses a flat three-argument signature to match the runtime slot.

Process spawning is a native capability; `--sandbox` rejects it. WebAssembly, mobile archive and Python-host targets do not provide it.
