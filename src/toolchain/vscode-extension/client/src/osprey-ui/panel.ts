// Implements [WORKBENCH-VSIX] [PACKAGE-EDITOR]: one native application shell for panel lifetime,
// the live instance one worker thread holds, frame publication and bounded host capabilities.

import { randomBytes } from "node:crypto";
import * as fs from "node:fs";
import * as os from "node:os";
import { ColorThemeKind, commands, env, Uri, ViewColumn, window, type ExtensionContext, type WebviewPanel } from "vscode";
import type { Ask, Completion } from "./asks";
import { createAssetResolver } from "./local-files";
import { renderWorkspace } from "./render";
import { resolveSymbols } from "./symbol-assets";
import { createWorkspaceSession, type SessionDeps, type WorkspaceSession } from "./session";
import { compileWasi } from "./wasi";
import { startNativeRuntime, type OpenOptions } from "./wasi-runner";
import { workspaceHtml } from "./webview-html";

export interface NativeApplication {
  readonly key: string;
  readonly artifact: string;
  readonly context: OpenOptions;
  readonly execute: (ask: Ask, slices: ReadonlyMap<string, string>) => Promise<Completion>;
  /** An approved directory of images, sprites and symbols the application names in its tree. */
  readonly assets?: string;
  /** The application's own stylesheet inside its asset root. */
  readonly stylesheet?: string;
  /** The application's symbol registry inside its asset root. */
  readonly symbols?: string;
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

function external(raw: unknown, title: string): void {
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) return;
  const url = (raw as Record<string, unknown>).url;
  if (typeof url !== "string") return;
  try {
    const opened = new URL(url);
    if (opened.protocol !== "https:" || opened.username || opened.password) throw new Error("Refused a non-HTTPS external link");
    void env.openExternal(Uri.parse(opened.toString()));
  } catch (error) { void window.showErrorMessage(`${title}: ${error instanceof Error ? error.message : String(error)}`); }
}

function receive(raw: unknown, active: ActivePanel, app: NativeApplication, title: string): void {
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) return;
  const message = raw as Record<string, unknown>;
  if (message.type === "ready") {
    void active.session.start({ ...app.context, now: Math.floor(Date.now() / 1000), dark: isDark(window.activeColorTheme.kind) });
    return;
  }
  if (message.type === "external") {
    external(message, title);
    return;
  }
  active.session.message(raw);
}

/** Every named asset source, resolved inside the approved asset root; a source outside it
 * resolves to nothing and draws its placeholder. */
function assetSources(resolve: (source: string) => string, sources: readonly string[]): Record<string, string> {
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

function sessionDeps(panel: WebviewPanel, app: NativeApplication, module: WebAssembly.Module, icon: string, loading: string, signal: AbortSignal): SessionDeps {
  const resolve = app.assets === undefined ? undefined
    : createAssetResolver(app.assets, (file) => panel.webview.asWebviewUri(Uri.file(file)).toString());
  return {
    runtime: startNativeRuntime(module, signal),
    send: (message) => { void panel.webview.postMessage(message); },
    execute: app.execute,
    draw: (frame, regions, slices) => renderWorkspace({ regions, slices, theme: frame.theme,
      assetUri: icon, resolveAsset: resolve, symbols: symbolsOf(app, resolve), loadingText: loading }),
    ...(resolve === undefined ? {} : { assets: (sources: readonly string[]) => assetSources(resolve, sources) }),
    report: (error) => console.error("Osprey native application:", error),
  };
}

/** The application's symbol registry, resolved through its approved asset root. */
function symbolsOf(app: NativeApplication, resolve?: (source: string) => string): ReadonlyMap<string, string> | undefined {
  if (app.assets === undefined || app.symbols === undefined || resolve === undefined) return undefined;
  return resolveSymbols(createAssetResolver(app.assets, file => fs.readFileSync(file, "utf8"))(app.symbols), resolve);
}

function document(context: ExtensionContext, panel: WebviewPanel, app: NativeApplication, options: NativePanelOptions): string {
  const stylesheetUri = app.assets !== undefined && app.stylesheet !== undefined
    ? createAssetResolver(app.assets, (file) => panel.webview.asWebviewUri(Uri.file(file)).toString())(app.stylesheet) : undefined;
  return workspaceHtml({ nonce: randomBytes(18).toString("base64"),
    assetUri: panel.webview.asWebviewUri(Uri.joinPath(context.extensionUri, "icon.png")).toString(),
    scriptUri: panel.webview.asWebviewUri(Uri.joinPath(context.extensionUri, "out", "webview", "webview", "main.js")).toString(),
    cspSource: panel.webview.cspSource, loadingText: options.loading, stylesheetUri });
}

function createPanel(context: ExtensionContext, app: NativeApplication, module: WebAssembly.Module, options: NativePanelOptions): ActivePanel {
  const panel = window.createWebviewPanel(options.view, options.title, ViewColumn.Active,
    { enableScripts: true, retainContextWhenHidden: true, localResourceRoots: [context.extensionUri] });
  try { return initializePanel(context, app, module, options, panel); }
  catch (error) { panel.dispose(); throw error; }
}

function initializePanel(context: ExtensionContext, app: NativeApplication, module: WebAssembly.Module, options: NativePanelOptions, panel: WebviewPanel): ActivePanel {
  panel.iconPath = Uri.joinPath(context.extensionUri, "icon.png");
  const icon = panel.webview.asWebviewUri(panel.iconPath).toString();
  const cancellation = new AbortController();
  const session = createWorkspaceSession(sessionDeps(panel, app, module, icon, options.loading, cancellation.signal));
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
  panel.webview.onDidReceiveMessage((raw) => receive(raw, active, app, options.title));
  panel.webview.html = document(context, panel, app, options);
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
