// Inline heat decorations for profiled source ([PROF-VSCODE-HEAT]): after-line
// annotations (" ▍ 12.4% · 124 samples") colored by heat, plus overview-ruler
// marks, driven by the summary's hotLines. The color/label/grouping decisions
// are PURE exported helpers; only HeatDecorationManager touches vscode.

import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";
import type { HotLineInfo, ProfileSummary } from "./summary";

export const HEAT_HIGH = "#e8500a"; // >= 20% of samples on one line
export const HEAT_WARM = "#f97316"; // >= 10%
export const HEAT_MILD = "#fbbf24"; // >= 5%
export const HEAT_MUTED = "#88888899"; // everything cooler

const HIGH_PCT = 20;
const WARM_PCT = 10;
const MILD_PCT = 5;

/** Heat tier color for a line's sample percentage. */
export function heatColor(pct: number): string {
  if (pct >= HIGH_PCT) {
    return HEAT_HIGH;
  }
  if (pct >= WARM_PCT) {
    return HEAT_WARM;
  }
  return pct >= MILD_PCT ? HEAT_MILD : HEAT_MUTED;
}

/** The after-line annotation text: ` ▍ 12.4% · 124 samples`. */
export function heatLabel(line: HotLineInfo): string {
  return ` ▍ ${line.pct.toFixed(1)}% · ${line.samples} samples`;
}

export interface LineHeat {
  line: number;
  label: string;
  color: string;
}

/** Resolves a path to its symlink-free form; throws when it does not exist. */
export type RealPathFn = (filePath: string) => string;

const nativeRealPath: RealPathFn = (filePath) =>
  fs.realpathSync.native(filePath);

/**
 * Canonical form of a file path for heat matching: normalized, symlinks
 * resolved when the path exists (macOS `/tmp` IS a symlink to `/private/tmp`,
 * so the profiler's paths and the editor's can disagree), lower-cased on
 * win32 where the filesystem is case-insensitive. Pure given its injections.
 */
export function canonicalHeatPath(
  filePath: string,
  realpath: RealPathFn = nativeRealPath,
  platform: NodeJS.Platform = process.platform,
): string {
  const normalized = path.normalize(filePath);
  let resolved = normalized;
  try {
    resolved = realpath(normalized);
  } catch {
    // Path does not exist (or realpath failed) — the normalized form still
    // lets relative/`..`-laden spellings of the same missing file match.
  }
  return platform === "win32" ? resolved.toLowerCase() : resolved;
}

/** The heat annotations that belong to one file (1-based lines). */
export function fileHeat(
  hotLines: HotLineInfo[],
  file: string,
  canonical: (filePath: string) => string = canonicalHeatPath,
): LineHeat[] {
  const target = canonical(file);
  return hotLines
    .filter((hot) => canonical(hot.file) === target)
    .map((hot) => ({
      line: hot.line,
      label: heatLabel(hot),
      color: heatColor(hot.pct),
    }));
}

/** Group annotations by tier color (one decoration type per color). */
export function groupByColor(heats: LineHeat[]): Map<string, LineHeat[]> {
  const groups = new Map<string, LineHeat[]>();
  for (const heat of heats) {
    const group = groups.get(heat.color) ?? [];
    group.push(heat);
    groups.set(heat.color, group);
  }
  return groups;
}

/** All tier colors — every editor gets each type set (possibly to []). */
export const HEAT_COLORS = [HEAT_HIGH, HEAT_WARM, HEAT_MILD, HEAT_MUTED];

function decorationOption(
  heat: LineHeat,
  lineCount: number,
): vscode.DecorationOptions | undefined {
  const line = heat.line - 1;
  if (line < 0 || line >= lineCount) {
    return undefined;
  }
  return {
    range: new vscode.Range(line, 0, line, 0),
    renderOptions: { after: { contentText: heat.label } },
  };
}

const readInlineHeatSetting = (): boolean =>
  vscode.workspace
    .getConfiguration("osprey")
    .get<boolean>("profiler.inlineHeat", true);

/**
 * Owns the decoration types and keeps visible editors annotated with the most
 * recent profile. `isEnabled` is injectable so the disabled branch is testable
 * without mutating user settings.
 */
export class HeatDecorationManager {
  private readonly types = new Map<string, vscode.TextEditorDecorationType>();
  private summary: ProfileSummary | undefined;

  public constructor(
    private readonly isEnabled: () => boolean = readInlineHeatSetting,
  ) {}

  /** The active summary (for tests/inspection). */
  public current(): ProfileSummary | undefined {
    return this.summary;
  }

  /** Adopt a new run's summary and annotate the visible editors. */
  public apply(
    summary: ProfileSummary,
    editors?: readonly vscode.TextEditor[],
  ): void {
    this.summary = summary;
    this.refresh(editors);
  }

  /** Re-apply (or strip, when disabled/cleared) on the visible editors. */
  public refresh(
    editors: readonly vscode.TextEditor[] = vscode.window.visibleTextEditors,
  ): void {
    for (const editor of editors) {
      this.decorateEditor(editor);
    }
  }

  /** Drop the current profile's annotations (a new run is starting). */
  public clear(editors?: readonly vscode.TextEditor[]): void {
    this.summary = undefined;
    this.refresh(editors);
  }

  public dispose(): void {
    this.types.forEach((type) => type.dispose());
    this.types.clear();
  }

  private decorateEditor(editor: vscode.TextEditor): void {
    const heats =
      this.summary !== undefined && this.isEnabled()
        ? fileHeat(this.summary.hotLines, editor.document.fileName)
        : [];
    const grouped = groupByColor(heats);
    for (const color of HEAT_COLORS) {
      const options = (grouped.get(color) ?? [])
        .map((heat) => decorationOption(heat, editor.document.lineCount))
        .filter(
          (option): option is vscode.DecorationOptions => option !== undefined,
        );
      editor.setDecorations(this.typeFor(color), options);
    }
  }

  private typeFor(color: string): vscode.TextEditorDecorationType {
    const existing = this.types.get(color);
    if (existing !== undefined) {
      return existing;
    }
    const type = vscode.window.createTextEditorDecorationType({
      isWholeLine: true,
      after: { color, margin: "0 0 0 1em", fontStyle: "italic" },
      overviewRulerColor: color,
      overviewRulerLane: vscode.OverviewRulerLane.Right,
      rangeBehavior: vscode.DecorationRangeBehavior.ClosedClosed,
    });
    this.types.set(color, type);
    return type;
  }
}
