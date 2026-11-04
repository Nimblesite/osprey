import * as assert from "assert";
import { execFile } from "child_process";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import { runFrontendCommand, semanticSuccess, FrontendSpawnDeps } from "../../client/src/package-frontend";

function runner(script: string): FrontendSpawnDeps {
  return { execFile: ((_executable: string, _args: unknown, options: unknown, callback: unknown) =>
    execFile(process.execPath, [script], options as any, callback as any)) as typeof execFile };
}

async function withProcess(source: string, run: (root: string, spawn: FrontendSpawnDeps) => Promise<void>) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "osprey package client "));
  const script = path.join(root, "frontend.cjs");
  fs.writeFileSync(script, source);
  try { await run(root, runner(script)); }
  finally { fs.rmSync(root, { recursive: true, force: true }); }
}

suite("package real subprocess protocol", () => {
  test("stdin and a workspace path containing spaces survive a real process", async () => {
    const source = `let input = ''; process.stdin.on('data', chunk => input += chunk);
      process.stdin.on('end', () => console.log(JSON.stringify({status:0,lines:[input.trim(),process.cwd()]})));`;
    await withProcess(source, async (root, spawn) => {
      const report = await runFrontendCommand("frontend", "--json add @org/package --project .", root, spawn);
      assert.deepStrictEqual(report, { status: 0, lines: ["--json add @org/package --project .", fs.realpathSync(root)] });
    });
  });
  test("semantic refusal remains a failure even when the process exits zero", async () => {
    await withProcess('process.stdin.resume(); console.log(JSON.stringify({status:1,lines:["add.exists"]}));', async (root, spawn) => {
      const report = await runFrontendCommand("frontend", "--json add @org/package", root, spawn);
      assert.strictEqual(semanticSuccess(report), false);
      assert.deepStrictEqual(report.lines, ["add.exists"]);
    });
  });
  test("a real process failure overrides printed success", async () => {
    await withProcess('console.log(JSON.stringify({status:0,lines:["success"]})); process.exitCode=17;', async (root, spawn) => {
      await assert.rejects(runFrontendCommand("frontend", "--json add @org/package", root, spawn), /frontend failed/);
    });
  });
  test("a child closing stdin early rejects without crashing the extension", async () => {
    await withProcess('process.exit(3);', async (root, spawn) => {
      await assert.rejects(runFrontendCommand("frontend", "x".repeat(1024 * 1024), root, spawn));
    });
  });
});
