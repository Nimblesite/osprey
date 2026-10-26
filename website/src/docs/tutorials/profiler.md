---
layout: page
video: profiler
permalink: /docs/tutorials/profiler/
eleventyComputed:
  title: "{{ (video | video).name }}"
  description: "Watch the Osprey CPU profiler in VS Code: run and profile the open file, read the flame graph and heat map, attach to a running program, and check for leaks with ARC."
tags: [tutorials, video, profiler, vscode, memory]
---

Osprey's VS Code extension has a CPU profiler built in. It samples a running Osprey program, shows where the time goes as a flame graph, and marks the hot lines in your source. This video shows the real extension profiling a real program. It then covers memory: how Osprey's backends work, and how to check a program for leaks.

{% videoPlayer video %}

## What the video covers

- **Run & Profile CPU.** In the Osprey Debug view, the Processes panel offers **Run & Profile CPU (Current File)**. It builds the open file, starts it under the debugger and samples it. While it runs, the panel and the status bar count the samples and name the hottest function so far: the one with the most samples of its own.
- **The results.** Click the counter in the status bar to stop. The results panel opens on its own, with a flame graph (left-heavy or in time order), the hot functions and the hot lines. Each hot line in the editor carries its share of the samples.
- **Attach to a running program.** Every Osprey program can be profiled while it runs, with no restart. Build it with `--profile` for line-level results, start it, and press the flame on its row in Processes. Hovering the row shows its CPU and memory.
- **Memory.** Osprey values never change once built, so the managed heap can never hold a reference cycle. You choose a memory backend when you build: `gc` is the default conservative tracing collector; `arc` releases a value when its last owner is dropped. Explicit `--memory=none` selects unmanaged allocation, which retains general allocations. Osprey has no live heap tracker yet, so the profiler offers no memory tracking.
- **Leaks.** Build with `--memory=arc` and run with `OSPREY_ARC_DEBUG=1`. When the program exits, ARC reports how many objects are still alive. `OSPREY_ARC_DEBUG=2` also lists each one by kind.

Every frame in the video is the real editor and language server. Both programs are in [`examples/profiling`](https://github.com/Nimblesite/osprey/tree/main/examples/profiling).

## Get it working

Install the Osprey extension and the compiler as described in [Install Osprey](/docs/installation/). Then, from a clone of the repository:

```sh
cd examples/profiling
osprey busy.osp --compile --profile -o busy    # a build whose samples carry source lines
./busy                                         # runs until stopped; profile it from Processes
```

Check a program for leaks:

```sh
osprey words.osp --compile --memory=arc -o words
OSPREY_ARC_DEBUG=1 ./words
# [osp-arc] exit: 0 live objects, 0 KiB (+0 immortal)
```

Osprey is actively evolving.

## Transcript

{% videoTranscript video %}
