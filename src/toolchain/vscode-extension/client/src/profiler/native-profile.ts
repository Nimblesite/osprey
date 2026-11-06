// Implements [PROF-VSCODE-SHARED]. Osprey metadata adapts to the shared results UI.
import type { ProfileResult } from "@nimblesite/lsp-profiler-ui";
import type { FlameModel } from "./flame-model";
import type { ProfileSummary } from "./summary";

export function nativeProfileResult(model: FlameModel, summary: ProfileSummary,
  artifacts?: { speedscope: string; cpuprofile: string }): ProfileResult {
  return {
    sessionId: summary.program, duration: summary.wallSeconds, totalSamples: summary.sampleCount,
    outputFile: artifacts?.speedscope ?? "", cpuProfilePath: artifacts?.cpuprofile,
    sampled: { model, summary },
    hotFunctions: summary.hotFunctions.map(fn => ({ name: fn.name,
      ...(fn.file ? { file: fn.file, line: fn.line } : {}), samples: fn.totalSamples,
      percentage: fn.totalPct, selfPercentage: fn.selfPct })),
    hotLines: summary.hotLines.map(line => ({ file: line.file, line: line.line,
      samples: line.samples, percentage: line.pct })),
  };
}
