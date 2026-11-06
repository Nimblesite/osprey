import type { ProfilerPalette } from "@nimblesite/lsp-profiler-ui/out/src/palette";

// Osprey host colors; the shared renderer owns their application.
export const OSPREY_PALETTE: ProfilerPalette = { critical: "#8b9254", hot: "#a39b67", warm: "#c0b587",
  memory: "#84916b", memoryHot: "#a4b084", memoryWarm: "#8e9b78", memoryCool: "#738263", leak: "#f87171", success: "#34d399" };
