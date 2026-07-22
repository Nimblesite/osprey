// Pure parsing/validation of `<stem>.profile.json` — the editor-integration
// summary written by `osprey --run --profile` ([PROF-CLI-RUN] step 4) — plus
// the formatting helpers the flame webview header and output channel share.
// No vscode imports.

import { parseJsonResult, type Result } from "./flame-model";

export interface FiberInfo {
  id: number;
  label: string;
  samples: number;
  oncpuSamples: number;
}

export interface HotFunctionInfo {
  name: string;
  file: string;
  line: number;
  selfPct: number;
  totalPct: number;
  selfSamples: number;
  totalSamples: number;
  kind: string;
}

export interface HotLineInfo {
  file: string;
  line: number;
  pct: number;
  samples: number;
}

export interface ProfileSummary {
  version: number;
  program: string;
  wallSeconds: number;
  cpuSeconds: number;
  sampleCount: number;
  rateHz: number;
  droppedSamples: number;
  fibers: FiberInfo[];
  hotFunctions: HotFunctionInfo[];
  hotLines: HotLineInfo[];
}

const NUMERIC_FIELDS = [
  "version",
  "wallSeconds",
  "cpuSeconds",
  "sampleCount",
  "rateHz",
] as const;

type RawRecord = Record<string, unknown>;

const isRecord = (value: unknown): value is RawRecord =>
  typeof value === "object" && value !== null && !Array.isArray(value);

function recordsIn(value: unknown): RawRecord[] {
  return Array.isArray(value) ? value.filter(isRecord) : [];
}

const num = (value: unknown): number => (typeof value === "number" ? value : 0);
const str = (value: unknown): string =>
  typeof value === "string" ? value : "";

function toFiber(raw: RawRecord): FiberInfo {
  return {
    id: num(raw.id),
    label: str(raw.label),
    samples: num(raw.samples),
    oncpuSamples: num(raw.oncpuSamples),
  };
}

function toHotFunction(raw: RawRecord): HotFunctionInfo {
  return {
    name: str(raw.name),
    file: str(raw.file),
    line: num(raw.line),
    selfPct: num(raw.selfPct),
    totalPct: num(raw.totalPct),
    selfSamples: num(raw.selfSamples),
    totalSamples: num(raw.totalSamples),
    kind: str(raw.kind),
  };
}

function toHotLine(raw: RawRecord): HotLineInfo {
  return {
    file: str(raw.file),
    line: num(raw.line),
    pct: num(raw.pct),
    samples: num(raw.samples),
  };
}

function normalizeSummary(raw: RawRecord): ProfileSummary {
  return {
    version: num(raw.version),
    program: str(raw.program),
    wallSeconds: num(raw.wallSeconds),
    cpuSeconds: num(raw.cpuSeconds),
    sampleCount: num(raw.sampleCount),
    rateHz: num(raw.rateHz),
    droppedSamples: num(raw.droppedSamples),
    fibers: recordsIn(raw.fibers).map(toFiber),
    hotFunctions: recordsIn(raw.hotFunctions)
      .map(toHotFunction)
      .filter((f) => f.name !== ""),
    hotLines: recordsIn(raw.hotLines)
      .map(toHotLine)
      .filter((l) => l.file !== "" && l.line > 0),
  };
}

/** Parse + validate a `<stem>.profile.json` document. Never throws. */
export function parseSummary(text: string): Result<ProfileSummary> {
  const raw = parseJsonResult(text, "profile summary");
  if (!raw.ok) {
    return raw;
  }
  if (!isRecord(raw.value)) {
    return { ok: false, error: "profile summary is not a JSON object" };
  }
  const record: RawRecord = raw.value;
  const missing = NUMERIC_FIELDS.find(
    (field) => typeof record[field] !== "number",
  );
  if (missing !== undefined) {
    return {
      ok: false,
      error: `profile summary field "${missing}" is missing or not a number`,
    };
  }
  return { ok: true, value: normalizeSummary(record) };
}

const formatSeconds = (seconds: number): string =>
  `${seconds >= 100 ? seconds.toFixed(0) : seconds.toFixed(1)}s`;

/** `4182 samples · 997Hz · 4.2s wall · 3.9s CPU · 3 fibers` (+ dropped). */
export function formatSummaryHeader(summary: ProfileSummary): string {
  const fibers = `${summary.fibers.length} ${summary.fibers.length === 1 ? "fiber" : "fibers"}`;
  const dropped =
    summary.droppedSamples > 0 ? ` · ${summary.droppedSamples} dropped` : "";
  return (
    `${summary.sampleCount} samples · ${summary.rateHz}Hz · ` +
    `${formatSeconds(summary.wallSeconds)} wall · ${formatSeconds(summary.cpuSeconds)} CPU · ` +
    `${fibers}${dropped}`
  );
}

/** A fiber's on-CPU share as a whole percentage (0 when it never sampled). */
export function onCpuPct(fiber: FiberInfo): number {
  return fiber.samples > 0
    ? Math.round((100 * fiber.oncpuSamples) / fiber.samples)
    : 0;
}
