// Implements [WORKBENCH-VSIX] [PACKAGE-EDITOR]: one native application shell
// for panel lifetime, WASI execution, rendering and bounded host capabilities.
import { randomBytes } from "node:crypto";
import * as fs from "node:fs";
import * as os from "node:os";
import { ColorThemeKind, commands, env, Uri, ViewColumn, window, type ExtensionContext, type WebviewPanel } from "vscode";
import { createAssetResolver } from "./local-files";
import { record } from "./protocol";
import { renderWorkspace } from "./render";
import { createWorkspaceSession, type SessionDeps, type WorkspaceSession } from "./session";
import { compileWasi, runWasi } from "./wasi";
import { workspaceHtml } from "./webview-html";
import { messageOf } from "./wire";
import { resolveSymbols } from "./symbol-assets";

export interface NativeApplication {
  readonly key: string;
  readonly artifact: string;
  readonly context: Record<string, unknown>;
  readonly execute: SessionDeps["execute"];
  readonly assets?: string;
  readonly stylesheet?: string;
  readonly symbols?: string;
}

export interface NativePanelOptions {
  readonly command: string;
  readonly view: string;
  readonly title: string;
  readonly loading: string;
  prepare(resource?: Uri): Promise<NativeApplication | undefined>;
}

interface ActivePanel { readonly key: string; readonly panel: WebviewPanel; readonly session: WorkspaceSession; }
interface PanelState {
  active?: ActivePanel;
  pending: Promise<void>;
  closed: boolean;
  readonly modules: Map<string, Promise<WebAssembly.Module>>;
}

/** Native application context, independent of project and UI decisions. */
export function nativeContext(arguments_: string[]): Record<string, unknown> {
  const kind = window.activeColorTheme.kind;
  return { platform: "vscode", now: Math.floor(Date.now() / 1000), offset: -new Date().getTimezoneOffset() * 60,
    home: os.homedir(), arguments: arguments_, theme: kind === ColorThemeKind.Light || kind === ColorThemeKind.HighContrastLight ? "light" : "dark" };
}

function external(raw: Record<string, unknown>, title: string): void {
  try {
    const url = new URL(typeof raw.url === "string" ? raw.url : "");
    if (url.protocol !== "https:" || url.username || url.password) throw new Error("Refused a non-HTTPS external link");
    void env.openExternal(Uri.parse(url.toString()));
  } catch (error) { void window.showErrorMessage(`${title}: ${messageOf(error)}`); }
}

function receive(raw: unknown, active: ActivePanel, app: NativeApplication, title: string): void {
  if (!record(raw)) return;
  if (raw.type === "ready") { void active.session.start(app.context); return; }
  if (raw.type === "external") { external(raw, title); return; }
  active.session.message(raw);
}

function applicationSession(panel: WebviewPanel, app: NativeApplication, module: WebAssembly.Module, icon: string, loadingText: string, signal: AbortSignal): WorkspaceSession {
  const resolveAsset = app.assets ? createAssetResolver(app.assets, file => panel.webview.asWebviewUri(Uri.file(file)).toString()) : undefined;
  const symbols = app.assets && app.symbols && resolveAsset
    ? resolveSymbols(createAssetResolver(app.assets, file => fs.readFileSync(file, "utf8"))(app.symbols), resolveAsset) : undefined;
  return createWorkspaceSession({
    run: frame => runWasi(module, frame, { signal }),
    render: (regions, _diagnostics, theme, slices) => renderWorkspace({ regions, slices, theme, assetUri: icon, resolveAsset, symbols, loadingText }),
    send: message => { void panel.webview.postMessage(message); },
    execute: app.execute,
    now: () => Math.floor(Date.now() / 1000),
    report: error => console.error("Osprey native application:", error),
  });
}

function document(context: ExtensionContext, panel: WebviewPanel, app: NativeApplication, options: NativePanelOptions): string {
  const stylesheetUri = app.assets && app.stylesheet
    ? createAssetResolver(app.assets, file => panel.webview.asWebviewUri(Uri.file(file)).toString())(app.stylesheet) : undefined;
  return workspaceHtml({ nonce: randomBytes(18).toString("base64"),
    assetUri: panel.webview.asWebviewUri(Uri.joinPath(context.extensionUri, "icon.png")).toString(),
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
  const active = { key: app.key, panel, session: applicationSession(panel, app, module, icon, options.loading, cancellation.signal) };
  panel.onDidDispose(() => { active.session.dispose(); cancellation.abort(); });
  panel.webview.onDidReceiveMessage(raw => receive(raw, active, app, options.title));
  panel.webview.html = document(context, panel, app, options);
  return active;
}

function disposeActive(state: PanelState): void {
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
      .catch(error => { void window.showErrorMessage(`${options.title}: ${messageOf(error)}`); });
    return state.pending;
  });
  const registration = { dispose: (): void => { state.closed = true; command.dispose(); disposeActive(state); state.modules.clear(); } };
  context.subscriptions.push(registration);
  return registration;
}
