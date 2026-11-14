// The real package registry for end-to-end package-manager tests ([PACKAGE-LOCAL-REGISTRY]): real
// Osprey projects published by the bundled `osprey-pkg`, served by the bundled `osprey-registry`
// over loopback. Nothing here answers a catalog request itself.
import * as assert from "assert";
import { execFileSync } from "child_process";
import * as fs from "fs";
import * as http from "http";
import * as os from "os";
import * as path from "path";
import { healthCheck, serverConfig, spawnServer, type ServerProcess } from "../../client/src/local-registry";
import { shipwrightPlatform } from "../../client/src/client-config";
import { ConfigurationTarget, workspace } from "vscode";

const bundle = path.resolve(__dirname, "..", "..", "..", "bin", shipwrightPlatform());
const tool = (name: string): string => path.join(bundle, process.platform === "win32" ? `${name}.exe` : name);
const license = fs.readFileSync(path.resolve(__dirname, "../../../../../../LICENSE"), "utf8");

/** One project to publish: `@scope/name`, and the package it depends on. */
export interface DemoPackage {
  readonly key: string;
  readonly dependency?: string;
}

/** The exact files a demo package's project holds, which an install must reproduce byte for byte. */
export function demoFiles(pkg: DemoPackage): { path: string; text: string }[] {
  const name = pkg.key.split("/")[1] ?? "";
  const manifest = `[project]\nname = "${name}"\nsource_roots = ["src"]\n[package]\nname = "${pkg.key}"\nlicense = "MIT"\nsummary = "Browser fixture ${name}"\nkeywords = ["browser-e2e"]\n` +
    (pkg.dependency ? `[dependencies]\n"${pkg.dependency}" = { epoch = 1, min_release = 1 }\n` : "");
  return [
    { path: "LICENSE", text: license },
    { path: "osprey.toml", text: manifest },
    { path: "src/main.osp", text: `namespace demo_${name.replace(/-/g, "_")};\nmodule Api { export fn value() = 42 }\n` },
  ];
}

/** Publishes `packages` into the registry tree at `root` with the real client. */
export function publish(root: string, packages: readonly DemoPackage[]): void {
  for (const pkg of packages) {
    const project = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-demo-project-"));
    try {
      for (const file of demoFiles(pkg)) {
        fs.mkdirSync(path.dirname(path.join(project, file.path)), { recursive: true });
        fs.writeFileSync(path.join(project, file.path), file.text);
      }
      const report = execFileSync(tool("osprey-pkg"), [], { input: `publish --folder ${root} --project ${project}\n`, encoding: "utf8", timeout: 30_000 });
      assert.ok(report.split(/\r?\n/).includes("status: 0"), `osprey-pkg publishes ${pkg.key}: ${report}`);
    } finally {
      fs.rmSync(project, { recursive: true, force: true });
    }
  }
}

function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const probe = http.createServer();
    probe.once("error", reject);
    probe.listen(0, "127.0.0.1", () => {
      const port = (probe.address() as { port: number }).port;
      probe.close(() => resolve(port));
    });
  });
}

/** A running `osprey-registry` over a registry tree it loaded at start. */
export class RealRegistry {
  private child: ServerProcess | undefined;
  private output = "";
  private constructor(readonly root: string, readonly port: number, private readonly cwd: string) {}

  get origin(): string { return `http://127.0.0.1:${this.port}`; }

  /** Publishes `packages` into a fresh tree and serves it. */
  static async start(packages: readonly DemoPackage[]): Promise<RealRegistry> {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-real-registry-"));
    publish(root, packages);
    const registry = new RealRegistry(root, await freePort(), fs.mkdtempSync(path.join(os.tmpdir(), "osprey-registry-cwd-")));
    await registry.serve();
    return registry;
  }

  /** Starts the server on this registry's port; it loads every release now on disk. */
  async serve(): Promise<void> {
    assert.strictEqual(this.child, undefined, "one server per registry");
    fs.writeFileSync(path.join(this.cwd, "registry-server.json"), serverConfig(this.root, "127.0.0.1", this.port));
    const child = spawnServer(tool("osprey-registry"), this.cwd);
    child.onOutput((text) => { this.output += text; });
    this.child = child;
    for (let attempt = 0; attempt < 100; attempt++) {
      if (await healthCheck(this.origin, 250)) return;
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    assert.fail(`osprey-registry never answered on ${this.origin}: ${this.output}`);
  }

  /** Stops the server and waits until its port no longer answers. */
  async stop(): Promise<void> {
    this.child?.kill();
    this.child = undefined;
    for (let attempt = 0; attempt < 100 && await healthCheck(this.origin, 250); attempt++) await new Promise((resolve) => setTimeout(resolve, 50));
    assert.strictEqual(await healthCheck(this.origin, 250), false, "the registry stopped");
  }

  /** Publishes more packages and restarts the server so it serves them. */
  async republish(packages: readonly DemoPackage[]): Promise<void> {
    await this.stop();
    publish(this.root, packages);
    await this.serve();
  }

  /** Everything the server printed: its served and refused releases, and every request. */
  log(): string { return this.output; }

  /** The digest the server lists for `key`'s release. */
  async digest(key: string): Promise<string> {
    const listing = await new Promise<string>((resolve, reject) => {
      http.get(`${this.origin}/v1/packages`, (response) => {
        let body = "";
        response.on("data", (chunk: Buffer) => { body += chunk.toString("utf8"); });
        response.on("end", () => resolve(body));
      }).on("error", reject);
    });
    const parsed = JSON.parse(listing) as { packages: { key: string; releases: { digest: string }[] }[] };
    const digest = parsed.packages.find((entry) => entry.key === key)?.releases[0]?.digest;
    assert.ok(digest, `the registry lists ${key}: ${listing}`);
    return digest;
  }

  async dispose(): Promise<void> {
    await this.stop();
    fs.rmSync(this.root, { recursive: true, force: true });
    fs.rmSync(this.cwd, { recursive: true, force: true });
  }
}

/** A hostile network hop in front of a real registry: it forwards every request and lets a test
 * rewrite the catalog body on its way back. The registry itself stays real. */
export async function tamperingHop(registry: RealRegistry, rewrite: () => ((body: string) => string) | undefined) {
  const server = http.createServer((request, response) => {
    const upstream = http.request(`${registry.origin}${request.url ?? "/"}`, { method: request.method, headers: request.headers }, (answer) => {
      let body = "";
      answer.on("data", (chunk: Buffer) => { body += chunk.toString("utf8"); });
      answer.on("end", () => {
        const change = request.url === "/v1/packages" ? rewrite() : undefined;
        const text = change ? change(body) : body;
        response.writeHead(answer.statusCode ?? 502, { "Content-Type": answer.headers["content-type"] ?? "application/json" });
        response.end(text);
      });
    });
    upstream.on("error", () => { response.writeHead(502); response.end(); });
    request.pipe(upstream);
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert.ok(address && typeof address !== "string");
  return { origin: `http://127.0.0.1:${address.port}`, close: () => new Promise<void>((resolve) => server.close(() => resolve())) };
}

/** Points the workspace at `origin` while `run` drives it. */
export async function withRegistrySetting(origin: string, run: () => Promise<void>): Promise<void> {
  const config = workspace.getConfiguration("osprey");
  const before = config.inspect<string>("packages.registry")?.workspaceValue;
  try {
    await config.update("packages.registry", origin, ConfigurationTarget.Workspace);
    await run();
  } finally {
    await config.update("packages.registry", before, ConfigurationTarget.Workspace);
  }
}
