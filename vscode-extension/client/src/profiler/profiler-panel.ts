// The singleton flame-graph WebviewPanel ([PROF-VSCODE-FLAME]): lspkit's
// results panel and message routing, titled after the profiled program.

import * as path from "path";
import * as vscode from "vscode";
import { SingletonWebviewPanel } from "@nimblesite/lsp-profiler-ui/out/src/profiler-webview";
import { buildFlamegraphHtml, handleFlamegraphMessage } from "@nimblesite/lsp-profiler-ui/out/src/profiler-flamegraph-html";
import { installIds } from "@nimblesite/lsp-profiler-ui/out/src/ids";
import { configurePalette, resolvePalette } from "@nimblesite/lsp-profiler-ui/out/src/palette";
import { nativeProfileResult } from "./native-profile";
import { OSPREY_PALETTE } from "./palette";
import type { FlameModel } from "./flame-model";
import type { ProfileSummary } from "./summary";

installIds({ idPrefix: "osprey", brand: "Osprey", configSection: "osprey", debugType: "osprey" });
configurePalette(() => resolvePalette(OSPREY_PALETTE,
  vscode.workspace.getConfiguration("osprey").get("profiler.palette")));

/** The one flame-graph panel; its `describe`/`html`/`isOpen` are the e2e seam. */
export const flamePanel = new SingletonWebviewPanel(() => "ospreyProfiler", handleFlamegraphMessage);

/** Show (or refresh) the singleton profiler panel for a new run. */
export async function showFlamePanel(
  model: FlameModel,
  summary: ProfileSummary,
  sourcePath: string,
  artifacts?: { speedscope: string; cpuprofile: string },
): Promise<void> {
  flamePanel.show(`Profile: ${path.basename(sourcePath)}`, await buildFlamegraphHtml(nativeProfileResult(model, summary, artifacts)));
}
