// The exact shared VSIX contract also runs inside SharpLsp.
import '@nimblesite/lsp-profiler-ui/out/src/testing/webview-contract';

import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import { execFile } from "child_process";
import { promisify } from "util";
import { registerCpuCaptureContract } from "@nimblesite/lsp-profiler-ui/out/src/testing/cpu-capture-contract";
import { artifactPaths, loadArtifacts } from "../../client/src/profiler/profile-run";
import { nativeProfileResult } from "../../client/src/profiler/native-profile";
import { extensionRoot, resolveBuiltOsprey } from "./osprey-test-env";

registerCpuCaptureContract(async () => {
  const compiler = resolveBuiltOsprey();
  assert.ok(compiler, "build the real Osprey compiler before running VSIX profiling tests");
  const source = path.resolve(extensionRoot, "../tests/regressions/fiber/cpu_profiling_demo.test.osp");
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-capture-contract-"));
  await promisify(execFile)(compiler, [source, "--profile"], {
    cwd, timeout: 100_000, env: { ...process.env, OSPREY_PROFILE_HZ: "8000" },
  });
  const artifacts = artifactPaths(cwd, source);
  const loaded = loadArtifacts(artifacts);
  assert.ok(loaded.ok, loaded.ok ? "" : loaded.error);
  return nativeProfileResult(loaded.value.model, loaded.value.summary, artifacts);
});
