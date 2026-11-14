// [PACKAGE-API] [PACKAGE-LOCAL-REGISTRY] end to end: the add and remove commands with the production
// deps — the bundled `osprey-pkg` mutating real projects against a real `osprey-registry` serving
// packages the real client published. Only the person's answers (picks and confirmations) are scripted.
import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import { ConfigurationTarget, workspace } from "vscode";
import { defaultPackageDeps, makeAddHandler, makeRemoveHandler, type PackageCommandDeps } from "../../client/src/package-commands";
import { fakeUi, recordQuickPicks, type RecordedUi, type UiScript } from "./package-harness";
import { extensionRoot } from "./osprey-test-env";
import { RealRegistry, demoFiles, type DemoPackage } from "./real-registry-kit";

const PACKAGES: readonly DemoPackage[] = [
  { key: "@demo/maths" }, { key: "@demo/greeting", dependency: "@demo/maths" }, { key: "@other/maths" },
];

/** A real, empty Osprey project in a folder whose path has spaces. */
function project(name: string): string {
  const root = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "osprey package e2e ")), name);
  fs.mkdirSync(path.join(root, "src"), { recursive: true });
  fs.writeFileSync(path.join(root, "osprey.toml"), `[project]\nname = "${name.replace(/\W/g, "_")}"\nsource_roots = ["src"]\n`);
  fs.writeFileSync(path.join(root, "src", "main.osp"), "fn main() = print(\"ok\")\n");
  return root;
}

const manifest = (root: string): string => fs.readFileSync(path.join(root, "osprey.toml"), "utf8");
const locked = (root: string): boolean => fs.existsSync(path.join(root, "osprey.lock"));

interface Live {
  readonly registry: RealRegistry;
  deps(roots: readonly string[], script?: UiScript): PackageCommandDeps & { ui: RecordedUi };
}

/** The production deps with the person's answers scripted, against a real registry. */
async function withLive(run: (live: Live) => Promise<void>, packages: readonly DemoPackage[] = PACKAGES): Promise<void> {
  const registry = await RealRegistry.start(packages);
  const config = workspace.getConfiguration("osprey");
  // The runner's profile is isolated, so its user settings name this registry for the run.
  const before = config.inspect<string>("packages.registry")?.globalValue;
  try {
    await config.update("packages.registry", registry.origin, ConfigurationTarget.Global);
    await run({ registry, deps: (roots, script) => ({ ...defaultPackageDeps(extensionRoot), ui: fakeUi(script), projectRoots: () => [...roots] }) });
  } finally {
    await config.update("packages.registry", before, ConfigurationTarget.Global);
    await registry.dispose();
  }
}

suite("package commands against the real registry", () => {
  test("add installs a real registry package and its dependency into a real project, and remove takes it out", async function () {
    this.timeout(60_000);
    const root = project("app");
    try {
      await withLive(async ({ registry, deps }) => {
        assert.ok(root.includes(" "), "the project path has spaces");
        const add = deps([root], { picks: [{ label: "@demo/greeting", key: "@demo/greeting" }] });
        add.ui.showQuickPick = async <T>(items: T[]) => items.find((item) => (item as { label?: string }).label === "@demo/greeting");
        await makeAddHandler(add)();
        assert.ok(add.ui.messages.some((message) => message.includes("added @demo/greeting")), add.ui.messages.join(" | "));
        assert.ok(manifest(root).includes('"@demo/greeting"'), manifest(root));
        assert.ok(!manifest(root).includes('"@demo/maths"'), "only the chosen package is a direct dependency");
        assert.ok(locked(root), "the real client locks the resolution");
        const lock = JSON.parse(fs.readFileSync(path.join(root, "osprey.lock"), "utf8")) as { packages: { release: { digest: string; package: { scope: string; name: string } } }[] };
        const lockedReleases = new Map(lock.packages.map((entry) => [`@${entry.release.package.scope}/${entry.release.package.name}`, entry.release.digest]));
        assert.deepStrictEqual([...lockedReleases.keys()].sort(), ["@demo/greeting", "@demo/maths"]);
        for (const key of ["@demo/greeting", "@demo/maths"]) {
          assert.strictEqual(lockedReleases.get(key), await registry.digest(key), `${key} is locked at the digest the registry serves`);
          const stored = path.join(root, ".osprey/packages", key, (await registry.digest(key)).slice(7));
          for (const file of demoFiles(PACKAGES.find((pkg) => pkg.key === key)!)) {
            assert.strictEqual(fs.readFileSync(path.join(stored, file.path), "utf8"), file.text, `${key} ${file.path} installed byte for byte`);
          }
        }
        assert.ok(registry.log().includes("/source"), "the sources came from the real registry");

        const remove = deps([root], { picks: [{ label: "@demo/greeting" }] });
        const offered: string[] = [];
        remove.ui.showQuickPick = async <T>(items: T[]) => {
          offered.push(...items.map((item) => String((item as { label?: string }).label)));
          return items[0];
        };
        await makeRemoveHandler(remove)();
        assert.deepStrictEqual(offered, ["@demo/greeting"], "only the direct dependency can be removed");
        assert.ok(!manifest(root).includes("@demo/greeting"), manifest(root));
      });
    } finally {
      fs.rmSync(path.dirname(root), { recursive: true, force: true });
    }
  });

  test("the real listing offers every org's package and warns that each is unverified", async () => {
    const root = project("listing");
    try {
      await withLive(async ({ deps }) => {
        const listing = deps([root]);
        const labels = recordQuickPicks(listing, "label");
        await makeAddHandler(listing)();
        assert.deepStrictEqual([...labels].sort(), ["@demo/greeting", "@demo/maths", "@other/maths"]);
        const details = deps([root]);
        const shown = recordQuickPicks(details, "detail");
        await makeAddHandler(details)();
        assert.strictEqual(shown.length, 3);
        for (const detail of shown) assert.match(detail, /unverified|security/i);
        assert.ok(!locked(root) && !manifest(root).includes("@"), "browsing changes nothing");
      });
    } finally {
      fs.rmSync(path.dirname(root), { recursive: true, force: true });
    }
  });

  test("declining the security confirmation leaves the real project untouched", async () => {
    const root = project("declined");
    const before = manifest(root);
    try {
      await withLive(async ({ registry, deps }) => {
        const declined = deps([root], { confirm: [false] });
        declined.ui.showQuickPick = async <T>(items: T[]) => items.find((item) => (item as { label?: string }).label === "@demo/maths");
        await makeAddHandler(declined)();
        assert.ok(declined.ui.messages.some((message) => /audit|security|unverified/i.test(message)), declined.ui.messages.join(" | "));
        assert.strictEqual(manifest(root), before);
        assert.ok(!locked(root) && !fs.existsSync(path.join(root, ".osprey")));
        assert.ok(!registry.log().includes("/source"), "nothing was downloaded");
      });
    } finally {
      fs.rmSync(path.dirname(root), { recursive: true, force: true });
    }
  });

  test("a stopped registry is reported and never launches the client", async () => {
    const root = project("offline");
    const before = manifest(root);
    try {
      await withLive(async ({ registry, deps }) => {
        await registry.stop();
        const offline = deps([root]);
        await makeAddHandler(offline)();
        assert.ok(offline.ui.messages.some((message) => message.startsWith("Could not read the registry listing")), offline.ui.messages.join(" | "));
        assert.strictEqual(manifest(root), before);
        assert.ok(!locked(root));
        await registry.serve();
      });
    } finally {
      fs.rmSync(path.dirname(root), { recursive: true, force: true });
    }
  });

  test("a multi-root selection installs into only the chosen real project", async function () {
    this.timeout(60_000);
    const first = project("first");
    const second = project("second");
    const untouched = manifest(first);
    try {
      await withLive(async ({ deps }) => {
        const add = deps([first, second]);
        let asked = 0;
        add.ui.showQuickPick = async <T>(items: T[]) => {
          asked += 1;
          return asked === 1 ? items[1] : items.find((item) => (item as { label?: string }).label === "@other/maths");
        };
        await makeAddHandler(add)();
        assert.ok(manifest(second).includes('"@other/maths"'), manifest(second));
        assert.ok(locked(second));
        assert.strictEqual(manifest(first), untouched, "the other project is never written");
        assert.ok(!locked(first));
      });
    } finally {
      for (const root of [first, second]) fs.rmSync(path.dirname(root), { recursive: true, force: true });
    }
  });
});
