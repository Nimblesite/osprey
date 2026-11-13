// Implements [WORKBENCH-VSIX] [PACKAGE-EDITOR]: one native application shell for panel lifetime,
// the live instance one worker thread holds, frame publication and bounded host capabilities.

import { randomBytes } from "node:crypto";
import * as fs from "node:fs";
import * as os from "node:os";
import { ColorThemeKind, commands, Uri, ViewColumn, window, type ExtensionContext, type WebviewPanel } from "vscode";
import type { Ask, Completion } from "./asks";
import { createAssetResolver } from "./local-files";
import { createWorkspaceSession, type SessionDeps, type WorkspaceSession } from "./session";
import { compileWasi } from "./wasi";
import { startNativeRuntime, type OpenOptions } from "./wasi-runner";
import { workspaceHtml } from "./webview-html";
import type { Shown } from "./webview/rows";

export interface NativeApplication {
  readonly key: string;
  readonly artifact: string;
  readonly context: OpenOptions;
  readonly execute: (ask: Ask, slices: ReadonlyMap<string, string>) => Promise<Completion>;
  /** An approved directory of images and sprites the application names in its rows. */
  readonly assets?: string;
}

export interface NativePanelOptions {
  readonly command: string;
  readonly view: string;
  readonly title: string;
  readonly loading: string;
  prepare(resource?: Uri): Promise<NativeApplication | undefined>;
}

interface ActivePanel { readonly key: string; readonly panel: WebviewPanel; readonly session: WorkspaceSession; readonly stop: () => void }
interface PanelState {
  active?: ActivePanel;
  pending: Promise<void>;
  closed: boolean;
  readonly modules: Map<string, Promise<WebAssembly.Module>>;
}

function isDark(kind: ColorThemeKind): boolean {
  return kind !== ColorThemeKind.Light && kind !== ColorThemeKind.HighContrastLight;
}

/** Native application context, independent of project and UI decisions. */
export function nativeContext(arguments_: readonly string[]): OpenOptions {
  return { platform: "vscode", now: Math.floor(Date.now() / 1000), offset: -new Date().getTimezoneOffset() * 60,
    home: os.homedir(), arguments: arguments_, dark: isDark(window.activeColorTheme.kind) };
}

function receive(raw: unknown, active: ActivePanel, app: NativeApplication): void {
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) return;
  const message = raw as Record<string, unknown>;
  if (message.type === "ready") {
    void active.session.start({ ...app.context, now: Math.floor(Date.now() / 1000), dark: isDark(window.activeColorTheme.kind) });
    return;
  }
  active.session.message(raw);
}

/** Every image and sprite a checked frame names, resolved inside the approved asset root. */
function imageSources(shown: Shown, resolve: (source: string) => string): Record<string, string> {
  const sources = new Set<string>();
  for (const control of shown.controls) {
    if (control.kind === "image" && control.words !== "") sources.add(control.words);
  }
  for (const scene of shown.scenes.values()) {
    for (const source of scene.atlas) if (source !== "") sources.add(source);
  }
  const assets: Record<string, string> = {};
  for (const source of sources) {
    try {
      assets[source] = resolve(source);
    } catch {
      // An unapproved or missing source draws its placeholder; nothing outside the root resolves.
    }
  }
  return assets;
}

function sessionDeps(panel: WebviewPanel, app: NativeApplication, module: WebAssembly.Module, signal: AbortSignal): SessionDeps {
  const resolve = app.assets === undefined ? undefined
    : createAssetResolver(app.assets, (file) => panel.webview.asWebviewUri(Uri.file(file)).toString());
  return {
    runtime: startNativeRuntime(module, signal),
    send: (message) => { void panel.webview.postMessage(message); },
    execute: app.execute,
    ...(resolve === undefined ? {} : { assets: (shown: Shown) => imageSources(shown, resolve) }),
    report: (error) => console.error("Osprey native application:", error),
  };
}

function document(context: ExtensionContext, panel: WebviewPanel, options: NativePanelOptions): string {
  return workspaceHtml({ nonce: randomBytes(18).toString("base64"),
    assetUri: panel.webview.asWebviewUri(Uri.joinPath(context.extensionUri, "icon.png")).toString(),
    scriptUri: panel.webview.asWebviewUri(Uri.joinPath(context.extensionUri, "out", "webview", "webview", "main.js")).toString(),
    cspSource: panel.webview.cspSource, loadingText: options.loading });
}

function createPanel(context: ExtensionContext, app: NativeApplication, module: WebAssembly.Module, options: NativePanelOptions): ActivePanel {
  const panel = window.createWebviewPanel(options.view, options.title, ViewColumn.Active,
    { enableScripts: true, retainContextWhenHidden: true, localResourceRoots: [context.extensionUri] });
  try { return initializePanel(context, app, module, options, panel); }
  catch (error) { panel.dispose(); throw error; }
}

function initializePanel(context: ExtensionContext, app: NativeApplication, module: WebAssembly.Module, options: NativePanelOptions, panel: WebviewPanel): ActivePanel {
  panel.iconPath = Uri.joinPath(context.extensionUri, "icon.png");
  const cancellation = new AbortController();
  const session = createWorkspaceSession(sessionDeps(panel, app, module, cancellation.signal));
  const theme = window.onDidChangeActiveColorTheme((current) => session.appearance(isDark(current.kind)));
  let stopped = false;
  const stop = (): void => {
    if (stopped) return;
    stopped = true;
    theme.dispose();
    session.dispose();
    cancellation.abort();
  };
  const active = { key: app.key, panel, session, stop };
  panel.onDidDispose(stop);
  panel.webview.onDidReceiveMessage((raw) => receive(raw, active, app));
  panel.webview.html = document(context, panel, options);
  return active;
}

function disposeActive(state: PanelState): void {
  state.active?.stop();
  state.active?.panel.dispose();
  state.active = undefined;
}

function loadModule(state: PanelState, artifact: string): Promise<WebAssembly.Module> {
  const module = state.modules.get(artifact) ?? fs.promises.readFile(artifact).then(compileWasi);
  state.modules.set(artifact, module);
  return module.catch(error => { state.modules.delete(artifact); throw error; });
}

async function openPanel(state: PanelState, context: ExtensionContext, options: NativePanelOptions, resource?: Uri): Promise<void> {
  if (state.closed) return;
  const app = await options.prepare(resource);
  if (!app || state.closed) return;
  if (state.active?.key === app.key) { state.active.panel.reveal(undefined, true); return; }
  const module = await loadModule(state, app.artifact);
  if (state.closed) return;
  disposeActive(state);
  const created = createPanel(context, app, module, options);
  state.active = created;
  created.panel.onDidDispose(() => { if (state.active === created) state.active = undefined; });
}

/** Register one native app with serialized opens and a single live panel. */
export function registerNativePanel(context: ExtensionContext, options: NativePanelOptions): { dispose(): void } {
  const state: PanelState = { pending: Promise.resolve(), modules: new Map(), closed: false };
  const command = commands.registerCommand(options.command, (resource?: Uri) => {
    state.pending = state.pending.then(() => openPanel(state, context, options, resource))
      .catch(error => { void window.showErrorMessage(`${options.title}: ${error instanceof Error ? error.message : String(error)}`); });
    return state.pending;
  });
  const registration = { dispose: (): void => { state.closed = true; command.dispose(); disposeActive(state); state.modules.clear(); } };
  context.subscriptions.push(registration);
  return registration;
}
