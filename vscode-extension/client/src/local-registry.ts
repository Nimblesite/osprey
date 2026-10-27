// The local package registry ([PACKAGE-LOCAL-REGISTRY]): the extension starts
// the bundled Osprey folder-registry server (`osprey-registry`) over the
// bundled snapshot of the repository's packages, so packages can be added
// before the hosted registry exists. The server is the same Osprey program
// `make registry-serve` runs; this module only writes its one-file
// configuration, owns the child process and answers "is it up?" through
// `GET /v1/health`. When the package manager is opened against a loopback
// registry that does not answer, the user is asked whether to start it.

import { ChildProcess, spawn } from "child_process";
import * as fs from "fs";
import * as http from "http";
import * as path from "path";
import { commands, ExtensionContext, StatusBarAlignment, Uri, window, workspace } from "vscode";
import { shipwrightPlatform } from "./client-config";

export const START_COMMAND = "osprey.packages.startLocalRegistry";
export const STOP_COMMAND = "osprey.packages.stopLocalRegistry";
export const START_ACTION = "Start Local Registry";
const DEFAULT_REGISTRY = "http://127.0.0.1:8787";
const CONFIG_FILE = "registry-server.json";
const loopbackHosts = ["127.0.0.1", "localhost", "[::1]", "::1"];

/** Where the server binary and the registry tree it serves live. */
export interface ServerLayout {
  executable: string;
  root: string;
}

/** The running server, as far as this module needs to know it. */
export interface ServerProcess {
  onOutput(listener: (text: string) => void): void;
  onExit(listener: (code: number | null) => void): void;
  kill(): void;
}

/** Everything the registry controller touches; every piece is injectable for tests. */
export interface LocalRegistryDeps {
  /** The configured registry origin for a resource. */
  registry(resource?: Uri): string;
  /** The bundled or configured server and registry tree. */
  layout(): ServerLayout;
  exists(file: string): boolean;
  /** Writes the server configuration into a private folder and returns that folder. */
  writeConfig(text: string): string;
  start(executable: string, cwd: string): ServerProcess;
  /** Whether `GET <origin>/v1/health` answers 200. */
  healthy(origin: string): Promise<boolean>;
  log(line: string): void;
  ask(message: string, action: string): Thenable<string | undefined>;
  error(message: string): void;
  info(message: string): void;
  /** Waits between health probes. */
  pause(ms: number): Promise<void>;
  /** Tells the status bar the state changed. */
  changed(state: RegistryState): void;
}

export type RegistryState = "stopped" | "starting" | "running" | "external";

/** A loopback origin and its port, or undefined for a remote registry. */
export function loopbackOrigin(origin: string): { origin: string; address: string; port: number } | undefined {
  let url: URL;
  try {
    url = new URL(origin);
  } catch {
    return undefined;
  }
  if (url.protocol !== "http:" || !loopbackHosts.includes(url.hostname)) return undefined;
  const port = url.port === "" ? 80 : Number(url.port);
  const address = url.hostname === "localhost" ? "127.0.0.1" : url.hostname.replace(/^\[|\]$/g, "");
  return { origin: url.origin, address, port };
}

/** The `registry-server.json` the Osprey server reads from its working directory. */
export function serverConfig(root: string, address: string, port: number): string {
  return `${JSON.stringify({ root, address, port }, null, 2)}\n`;
}

/** The server's own words for why it stopped, from its output. */
export function failureOf(output: string): string {
  const errors = output.split(/\r?\n/).filter((line) => line.startsWith("error:"));
  return errors.length ? errors.join("; ") : "the server exited before it answered";
}

/** Starts, stops and probes the one local registry server this window owns. */
export class LocalRegistry {
  private child: ServerProcess | undefined;
  private starting: Promise<boolean> | undefined;
  private state: RegistryState = "stopped";

  constructor(private readonly deps: LocalRegistryDeps, private readonly readyTimeoutMs = 120_000, private readonly probeMs = 250) {}

  get current(): RegistryState {
    return this.state;
  }

  private set(state: RegistryState): void {
    this.state = state;
    this.deps.changed(state);
  }

  /** Re-reads whether something answers on the configured origin. */
  async refresh(resource?: Uri): Promise<RegistryState> {
    if (this.starting) return this.state;
    const local = loopbackOrigin(this.deps.registry(resource));
    if (!local) return this.state;
    const up = await this.deps.healthy(local.origin);
    this.set(up ? (this.child ? "running" : "external") : this.child ? "starting" : "stopped");
    return this.state;
  }

  /** Starts the server unless something already answers; true once it answers. */
  start(resource?: Uri): Promise<boolean> {
    this.starting ??= this.launch(resource).finally(() => { this.starting = undefined; });
    return this.starting;
  }

  private async launch(resource?: Uri): Promise<boolean> {
    const local = loopbackOrigin(this.deps.registry(resource));
    if (!local) {
      this.deps.error(`The package registry ${this.deps.registry(resource)} is not a loopback HTTP origin, so there is no local server to start. Set osprey.packages.registry to ${DEFAULT_REGISTRY}.`);
      return false;
    }
    if (await this.deps.healthy(local.origin)) {
      this.set(this.child ? "running" : "external");
      return true;
    }
    const layout = this.deps.layout();
    for (const [what, file] of [["server", layout.executable], ["package snapshot", layout.root]] as const) {
      if (!this.deps.exists(file)) {
        // Windows builds do not bundle the server yet: its tree walk shells out to POSIX `find`.
        const remedy = process.platform === "win32" ? "This build does not bundle the local registry on Windows yet; run make registry-serve under WSL" : "Run make _vsix_bundle";
        this.deps.error(`The local registry ${what} is missing: ${file}. ${remedy}, or set osprey.packages.localRegistry.${what === "server" ? "serverPath" : "root"}.`);
        return false;
      }
    }
    const cwd = this.deps.writeConfig(serverConfig(layout.root, local.address, local.port));
    this.set("starting");
    this.deps.log(`Starting ${layout.executable} on ${local.origin} (root ${layout.root})`);
    const child = this.deps.start(layout.executable, cwd);
    this.child = child;
    let output = "";
    let exited = false;
    child.onOutput((text) => {
      output += text;
      this.deps.log(text.replace(/\n$/, ""));
    });
    child.onExit((code) => {
      exited = true;
      this.deps.log(`Local registry exited (${code ?? "signal"})`);
      if (this.child === child) {
        this.child = undefined;
        this.set("stopped");
      }
    });
    return this.awaitReady(local.origin, child, () => exited, () => output);
  }

  private async awaitReady(origin: string, child: ServerProcess, exited: () => boolean, output: () => string): Promise<boolean> {
    for (let waited = 0; waited < this.readyTimeoutMs; waited += this.probeMs) {
      if (exited()) {
        this.deps.error(`The local package registry did not start: ${failureOf(output())}.`);
        return false;
      }
      if (await this.deps.healthy(origin)) {
        this.set("running");
        this.deps.info(`Local package registry running at ${origin}.`);
        return true;
      }
      await this.deps.pause(this.probeMs);
    }
    child.kill();
    this.deps.error(`The local package registry did not answer ${origin}/v1/health within ${Math.round(this.readyTimeoutMs / 1000)} s.`);
    return false;
  }

  /** Stops the server this window started; a server started elsewhere is left alone. */
  stop(): boolean {
    const child = this.child;
    if (!child) return false;
    this.child = undefined;
    child.kill();
    this.set("stopped");
    return true;
  }

  /**
   * True when the registry is usable: remote registries are the user's to run,
   * a loopback one that answers is used as is, and a silent one is offered to
   * start.
   */
  async ensureRunning(resource?: Uri): Promise<boolean> {
    const origin = this.deps.registry(resource);
    const local = loopbackOrigin(origin);
    if (!local || await this.deps.healthy(local.origin)) return true;
    const answer = await this.deps.ask(`The Osprey package registry at ${local.origin} is not running. Start the local registry?`, START_ACTION);
    return answer === START_ACTION ? this.start(resource) : false;
  }
}

/** GET /v1/health, bounded; any failure is "not healthy". */
export function healthCheck(origin: string, timeoutMs = 2_000): Promise<boolean> {
  return new Promise((resolve) => {
    const request = http.get(new URL("/v1/health", origin), (response) => {
      response.resume();
      resolve(response.statusCode === 200);
    });
    request.setTimeout(timeoutMs, () => request.destroy());
    request.once("error", () => resolve(false));
  });
}

/** The server as a real child process, its output and exit forwarded. */
export function spawnServer(executable: string, cwd: string): ServerProcess {
  const child: ChildProcess = spawn(executable, [], { cwd, stdio: ["ignore", "pipe", "pipe"], windowsHide: true });
  return {
    onOutput: (listener) => {
      child.stdout?.on("data", (chunk: Buffer) => listener(chunk.toString("utf8")));
      child.stderr?.on("data", (chunk: Buffer) => listener(chunk.toString("utf8")));
    },
    onExit: (listener) => {
      child.once("exit", (code) => listener(code));
      child.once("error", () => listener(-1));
    },
    kill: () => { child.kill(); },
  };
}

/** The bundled server and snapshot, unless settings name others. */
export function bundledLayout(extensionPath: string, platform = shipwrightPlatform()): ServerLayout {
  const config = workspace.getConfiguration("osprey");
  const dir = path.join(extensionPath, "bin", platform);
  const executable = process.platform === "win32" ? "osprey-registry.exe" : "osprey-registry";
  return {
    executable: config.get<string>("packages.localRegistry.serverPath") || path.join(dir, executable),
    root: config.get<string>("packages.localRegistry.root") || path.join(dir, "registry"),
  };
}

function registryFor(resource?: Uri): string {
  return workspace.getConfiguration("osprey", resource).get<string>("packages.registry") || DEFAULT_REGISTRY;
}

const labels: Record<RegistryState, { text: string; tooltip: string; command: string }> = {
  stopped: { text: "$(package) Registry: off", tooltip: "Osprey local package registry is not running. Click to start it.", command: START_COMMAND },
  starting: { text: "$(loading~spin) Registry: starting", tooltip: "Osprey local package registry is loading its packages.", command: STOP_COMMAND },
  running: { text: "$(package) Registry: on", tooltip: "Osprey local package registry is running. Click to stop it.", command: STOP_COMMAND },
  external: { text: "$(package) Registry: on", tooltip: "An Osprey package registry is already answering on the configured origin.", command: START_COMMAND },
};

/** Registers the start/stop commands, the status-bar action and process cleanup. */
export function registerLocalRegistry(context: ExtensionContext): LocalRegistry {
  const channel = window.createOutputChannel("Osprey Package Registry");
  const item = window.createStatusBarItem("osprey.packages.localRegistry", StatusBarAlignment.Left, 0);
  item.name = "Osprey Package Registry";
  const show = (state: RegistryState): void => {
    Object.assign(item, labels[state]);
    item.show();
  };
  const storage = context.globalStorageUri.fsPath;
  const registry = new LocalRegistry({
    registry: registryFor,
    layout: () => bundledLayout(context.extensionPath),
    exists: (file) => fs.existsSync(file),
    writeConfig: (text) => {
      const dir = path.join(storage, "local-registry");
      fs.mkdirSync(dir, { recursive: true });
      fs.writeFileSync(path.join(dir, CONFIG_FILE), text);
      return dir;
    },
    start: spawnServer,
    healthy: (origin) => healthCheck(origin),
    log: (line) => channel.appendLine(line),
    ask: (message, action) => window.showInformationMessage(message, { modal: true }, action),
    error: (message) => {
      channel.show(true);
      void window.showErrorMessage(message);
    },
    info: (message) => void window.showInformationMessage(message),
    pause: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
    changed: show,
  });
  show("stopped");
  void registry.refresh();
  context.subscriptions.push(
    channel,
    item,
    commands.registerCommand(START_COMMAND, () => registry.start()),
    commands.registerCommand(STOP_COMMAND, () => {
      if (!registry.stop()) void window.showInformationMessage("No local package registry was started from this window.");
    }),
    { dispose: () => { registry.stop(); } },
  );
  return registry;
}
