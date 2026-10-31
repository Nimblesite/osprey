// Tests for [PROF-VSCODE-SHARED]. The seams every live profiler suite shares,
// ported from SharpLsp's profiler kit and bound to Osprey: the shared UI's
// store through the extension API, the real `osprey lsp` client, the wire
// commands the library drives, and real Osprey programs compiled with debug
// info and attached through their live-attach endpoint [PROF-ATTACH-ENDPOINT].
import * as assert from "node:assert/strict";
import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import * as vscode from "vscode";
import { chromium, errors, type Browser, type Frame } from "playwright-core";
import type { LanguageClient } from "vscode-languageclient/node";
import { profilerStatusText, type ProcessesProvider, type ProfilerStore } from "@nimblesite/lsp-profiler-ui";
import type { ProcessList, ProfileReport, StartProfileResult } from "@nimblesite/lsp-profiler-ui/out/src/wire-generated";
import { closeAllEditors, pollUntilResult, removeTestDir } from "@nimblesite/lsp-profiler-ui/out/src/testing/webview-contract-helpers";
import type { OspreyExtensionApi } from "../../client/src/extension";
import { extensionRoot, resolveBuiltOsprey } from "./osprey-test-env";

export async function connectEditorBrowser(): Promise<Browser> {
  const userData = process.env.OSPREY_TEST_USER_DATA;
  assert.ok(userData, "runner supplies its isolated browser profile");
  const portFile = path.join(userData, "DevToolsActivePort");
  await waitFor(() => fs.existsSync(portFile), Boolean, 8000, "editor browser debugging endpoint");
  const port = Number(fs.readFileSync(portFile, "utf8").split("\n")[0]);
  assert.ok(Number.isInteger(port) && port > 0);
  const browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
  for (const context of browser.contexts()) context.setDefaultTimeout(3000);
  return browser;
}


const FRAME_PROBE_MS = 100;

async function frameMatches(frame: Frame, selector: string): Promise<boolean> {
  if (frame.isDetached()) return false;
  try {
    await frame.locator(selector).waitFor({ state: "attached", timeout: FRAME_PROBE_MS });
    return true;
  } catch (error) {
    if (error instanceof errors.TimeoutError || frame.isDetached()) return false;
    throw error;
  }
}

export async function editorWebviewFrame(browser: Browser, selector: string): Promise<Frame> {
  const frame = await pollUntilResult({
    fn: async () => {
      for (const candidate of browser.contexts().flatMap((context) => context.pages()).flatMap((page) => page.frames())) {
        if (await frameMatches(candidate, selector)) return candidate;
      }
      return undefined;
    },
    until: (candidate) => candidate !== undefined,
    timeoutMs: 5000,
    description: `actual editor webview matching ${selector}`,
  });
  assert.ok(frame);
  return frame;
}

export const EXTENSION_ID = "nimblesite.osprey";

// ── Timeouts: ceilings, never budgets ─────────────────────────────

/** One command round trip through the extension host. */
export const COMMAND_MS = 5_000;
/** One profiler request answered by the running server. */
export const LSP_RESPONSE_MS = 15_000;
/** A compiled Osprey program starting and opening its endpoint. */
export const PROCESS_START_MS = 30_000;
/** Activating the extension and its language server: suite hooks only. */
export const ACTIVATION_MS = 60_000;
/** Compiling the fixture with debug info: suite hooks only. */
export const BUILD_MS = 120_000;

// ── The extension under test ──────────────────────────────────────

/** The profiler's wire commands as `osprey lsp` advertises them [PROF-LSP-HOST]. */
export function lspCommand(name: string): string {
  return `osprey.lspProfiler.${name}`;
}

/** The library's client-side commands under Osprey's prefix. */
export function uiCommand(name: string): string {
  return `osprey.profiler.${name}`;
}

function api(): OspreyExtensionApi | undefined {
  return vscode.extensions.getExtension<OspreyExtensionApi>(EXTENSION_ID)?.exports;
}

/** The live profiler store the extension publishes. */
export function profilerStore(): ProfilerStore {
  const store = api()?.profiler.ui.store;
  assert.ok(store, "the extension must publish the profiler store");
  return store;
}

/** The Processes panel's data provider: the rows the panel renders. */
export function processesProvider(): ProcessesProvider {
  const provider = api()?.profiler.ui.processes;
  assert.ok(provider, "the extension must publish the processes provider");
  return provider;
}

/** The running language client. */
export function lspClient(): LanguageClient {
  const client = api()?.getLspClient();
  assert.ok(client !== undefined && client.isRunning(), "the Osprey language client must be running");
  return client;
}

/**
 * Point the extension at this build's compiler, activate it, and wait until
 * `osprey lsp` answers and the profiler holds its client.
 */
export async function activateWithServer(): Promise<void> {
  const compiler = resolveBuiltOsprey();
  assert.ok(compiler, "build the Osprey compiler (target/release/osprey) before the profiler suites");
  const extension = vscode.extensions.getExtension<OspreyExtensionApi>(EXTENSION_ID);
  assert.ok(extension, `${EXTENSION_ID} must be installed in the test host`);
  if (!extension.isActive) {
    await vscode.workspace.getConfiguration("osprey").update("server.compilerPath", compiler, vscode.ConfigurationTarget.Global);
    await extension.activate();
  }
  await waitFor(() => profilerStore().client.value?.isRunning() === true, (running) => running,
    ACTIVATION_MS - 10_000, "the profiler's language client running");
}

/** Poll until `until` accepts `read()`'s value; fails naming `what` and the last value. */
export async function waitFor<T>(read: () => T, until: (value: T) => boolean, timeoutMs: number, what: string): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  let last = read();
  while (!until(last)) {
    assert.ok(Date.now() < deadline, `${what} not reached within ${timeoutMs}ms (last: ${JSON.stringify(last)})`);
    await new Promise((resolve) => setTimeout(resolve, 100));
    last = read();
  }
  return last;
}

// ── The wire ──────────────────────────────────────────────────────

/** Run one profiler operation over `workspace/executeCommand` exactly as the library does. */
export async function execute<T>(name: string, params: Record<string, unknown>): Promise<T> {
  return lspClient().sendRequest<T>("workspace/executeCommand", { command: lspCommand(name), arguments: [params] });
}

/** The error a wire call rejects with, or `undefined` when it succeeded. */
export async function executeError(name: string, params: Record<string, unknown>): Promise<{ code: number; message: string } | undefined> {
  try {
    await execute(name, params);
    return undefined;
  } catch (error: unknown) {
    const failure = error as { code?: unknown; message?: unknown };
    return { code: typeof failure.code === "number" ? failure.code : 0, message: String(failure.message ?? error) };
  }
}

/** Assert one wire call is refused with exactly `code`; returns the refusal. */
export async function assertWireError(name: string, params: Record<string, unknown>, code: number, what: string): Promise<{ code: number; message: string }> {
  const error = await executeError(name, params);
  assert.ok(error !== undefined, `${what}: the call must be refused, not answered`);
  assert.equal(error.code, code, `${what}: ${error.message}`);
  return error;
}

/** Start a CPU session on `pid`, let it sample for `windowMs`, stop it; both replies. */
export async function sampleCpu(pid: number, windowMs: number, extra: Record<string, unknown> = {}): Promise<{ started: StartProfileResult; report: ProfileReport }> {
  const started = await execute<StartProfileResult>("cpu.start", { pid, ...extra });
  await new Promise((resolve) => setTimeout(resolve, windowMs));
  return { started, report: await execute<ProfileReport>("cpu.stop", { session_id: started.session_id }) };
}

// ── Osprey targets ────────────────────────────────────────────────

/** The live-attach fixture the Rust live suites run too: `spin` burns CPU far longer than any test. */
export const FIXTURE_SOURCE = path.resolve(extensionRoot, "../crates/osprey-cli/tests/fixtures/live_attach.osp");
/** `spin`, the fixture's hot function, from its signature through its last arm. */
export const HOT_FUNCTION = "spin";
export const HOT_LINES = { first: 9, last: 13 } as const;

/** Where compiled fixtures live for this test run, so each workspace-root test has a known directory. */
export const FIXTURE_DIR = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-profiler-fixtures-"));

/** Compile the fixture once per `name` with debug info (`--compile --profile`) and return the binary. */
export function buildTarget(name: string): string {
  const binary = path.join(FIXTURE_DIR, name);
  if (!fs.existsSync(binary)) {
    const compiler = resolveBuiltOsprey();
    assert.ok(compiler, "build the Osprey compiler before the profiler suites");
    execFileSync(compiler, [FIXTURE_SOURCE, "--compile", "--profile", "--quiet", "-o", binary],
      { cwd: path.resolve(extensionRoot, ".."), timeout: BUILD_MS, stdio: "pipe" });
  }
  assert.ok(fs.existsSync(binary), `${binary} missing after compiling ${FIXTURE_SOURCE}`);
  return binary;
}

/** A running Osprey program the server lists as attachable; `stop()` kills it. */
export class Target {
  private constructor(private readonly child: ChildProcess, readonly pid: number, readonly binary: string) {}

  /** Start `name` and resolve once `osprey lsp` lists it as an attachable Osprey process. */
  static async start(name: string): Promise<Target> {
    const binary = buildTarget(name);
    const child = spawn(binary, [], { stdio: "ignore" });
    assert.ok(child.pid !== undefined, `${name} must spawn`);
    const target = new Target(child, child.pid, binary);
    await waitForListing(target.pid);
    return target;
  }

  /** Whether the process still exists. */
  alive(): boolean {
    try {
      process.kill(this.pid, 0);
      return true;
    } catch {
      return false;
    }
  }

  stop(): void {
    this.child.kill("SIGKILL");
  }
}

/** Resolve once `osprey lsp` lists `pid` as an attachable Osprey process: its endpoint is open. */
export async function waitForListing(pid: number): Promise<void> {
  const deadline = Date.now() + PROCESS_START_MS;
  for (;;) {
    const listing = await execute<ProcessList>("processes", { workspace_roots: [] });
    if (listing.processes.some((proc) => proc.pid === pid && proc.attachable)) return;
    assert.ok(Date.now() < deadline, `PID ${pid} never listed as an attachable Osprey process`);
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}

/** What a suite adds around the shared lifecycle. */
export interface ProfilingSuiteHooks {
  /** Before the target stops: end a session a failed test left running. */
  readonly stopping?: () => PromiseLike<unknown>;
}

/**
 * Register the lifecycle every live suite shares: the extension and its server
 * running, then `name` running; afterwards stop it and close every editor.
 */
export function profilingSuite(name: string, hooks: ProfilingSuiteHooks = {}): { readonly target: Target } {
  let running: Target | undefined;
  suiteSetup(async function () {
    this.timeout(ACTIVATION_MS + BUILD_MS + PROCESS_START_MS);
    await activateWithServer();
    running = await Target.start(name);
  });
  suiteTeardown(async () => {
    await hooks.stopping?.();
    running?.stop();
    await closeAllEditors();
  });
  return {
    get target() {
      assert.ok(running, `${name} must be running`);
      return running;
    },
  };
}

/** The hooks a suite needs without a target: the server running, a scratch dir, editors closed after each test. */
export function useLspSuite(prefix: string, cleanup: () => void = () => undefined): () => string {
  let tmpDir = "";
  suiteSetup(async function () {
    this.timeout(ACTIVATION_MS);
    await activateWithServer();
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  });
  suiteTeardown(() => removeTestDir(tmpDir));
  teardown(async () => {
    cleanup();
    await closeAllEditors();
  });
  return () => tmpDir;
}

// ── Driving the UI ────────────────────────────────────────────────

/** Open `file` so decorations have an editor to land on. */
export async function openSource(file: string): Promise<vscode.TextEditor> {
  return vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file), { preview: false });
}

/** Write one `osprey.profiler.*` setting globally; `undefined` restores its default. */
export async function setProfilerSetting(key: string, value: unknown): Promise<void> {
  await vscode.workspace.getConfiguration("osprey.profiler").update(key, value, vscode.ConfigurationTarget.Global);
}

/** Profile `pid` from its Processes row; resolves once the session records. */
export async function profileFromRow(pid: number): Promise<void> {
  await vscode.commands.executeCommand(uiCommand("profileProcess"), { process: { pid, debuggable: true, runtime: "osprey" } });
  await waitFor(() => profilerStore().profiler.value.cpu, (cpu) => cpu === "active", PROCESS_START_MS, "CPU session active");
}

/** Stop CPU profiling through the real command; resolves once the session is idle. */
export async function stopProfiling(timeoutMs: number): Promise<void> {
  await vscode.commands.executeCommand(uiCommand("profileStop"));
  await waitFor(() => profilerStore().profiler.value.cpu, (cpu) => cpu === "idle", timeoutMs, "profileStop reached idle");
}

/** Profile `pid` from its row for `sampleMs`, asserting the live chrome, then stop it. */
export async function profileFromRowFor(pid: number, sampleMs: number, stopMs: number): Promise<void> {
  await profileFromRow(pid);
  assert.equal(profilerStore().profiler.value.cpuPid, pid, "the session profiles the target");
  assert.ok(profilerStatusText()?.includes("Profiling"), `status bar shows the live session: ${profilerStatusText()}`);
  await new Promise((resolve) => setTimeout(resolve, sampleMs));
  await stopProfiling(stopMs);
}

/** The top-level rows of the Processes panel as it renders them now. */
export function panelRows(): vscode.TreeItem[] {
  return processesProvider().getChildren();
}

/** The panel row of `pid`, searching inside group headers too. */
export function rowOf(pid: number): vscode.TreeItem | undefined {
  const provider = processesProvider();
  return provider.getChildren()
    .flatMap((item) => (item.contextValue === "processGroup" ? provider.getChildren(item) : [item]))
    .find((item) => item.id === `process:${pid}`);
}

/** A row's tooltip as plain text. */
export function tooltipText(item: vscode.TreeItem): string {
  const tip = item.tooltip;
  return typeof tip === "string" ? tip : (tip?.value ?? "");
}
