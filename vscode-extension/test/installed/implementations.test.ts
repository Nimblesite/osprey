// Go to Implementations / Find All Implementations, and Go to Definition,
// against the INSTALLED VSIX and its bundled compiler, over the repository's own example projects copied
// verbatim into the test workspace. Every case names a cursor position by its
// source text and asserts the EXACT set of locations the editor receives, so a
// missing handler, an extra one, or a range off by one column fails.
// [LSP-IMPLEMENTATIONS-EFFECT-HANDLERS] [LSP-DEFINITION-EFFECT-OPERATIONS]
// [LSP-DEFINITION-LOCAL]
//
// gpt2 is the Default flavor across three projects that share `../model`;
// modules is the ML flavor with a qualified handler value, a `handler` bound
// to a name, and an effect promised by a `signature`.

import * as assert from "assert";
import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";
import { closeEditors, rangeOf, waitFor } from "./editor";

/** `word` inside the `occurrence`-th match of `context` in `file`. */
interface Site {
  file: string;
  context: string;
  word: string;
  occurrence?: number;
}

interface Case {
  from: Site;
  expect: Site[];
}

const BACKENDS = "gpt2/model/backends.osp";
const CPU = "gpt2/model/cpu.osp";
const GPU = "gpt2/model/gpu.osp";
const MODEL = "gpt2/model/model.osp";
const GENERATE = "gpt2/generate/generate.osp";
const TRAIN = "gpt2/train/train.osp";
const LEDGER = "modules/src/store/ledger.ospml";
const METRICS = "modules/src/store/metrics.ospml";
const ROUTES = "modules/src/api/routes.ospml";
const MAIN = "modules/src/main.ospml";

/** Example directories copied into the workspace; gpt2's weights are not. */
const COPIES: [string, string][] = [
  ["gpt2/model", "gpt2/model"],
  ["gpt2/generate", "gpt2/generate"],
  ["gpt2/train", "gpt2/train"],
  ["projects/modules/osprey.toml", "modules/osprey.toml"],
  ["projects/modules/src", "modules/src"],
];

const site = (
  file: string,
  context: string,
  word: string,
  occurrence = 0,
): Site => ({ file, context, word, occurrence });

const occurrences = (count: number): number[] =>
  Array.from({ length: count }, (_, index) => index);

// --- gpt2 (Default): `Tensor` and its ten operations ------------------------

/** Which project files perform each `Tensor` operation. */
const TENSOR_PERFORMS: Record<string, string[]> = {
  build: [MODEL, GENERATE],
  get: [MODEL],
  length: [MODEL],
  zipWith: [MODEL, TRAIN],
  reduce: [MODEL, GENERATE],
  indexOf: [GENERATE],
  matMul: [MODEL],
  fromList: [MODEL, TRAIN],
  load: [MODEL],
  device: [MODEL],
};

const TENSOR_HEADS = [
  site(CPU, "handle static Tensor<List<float>>", "Tensor"),
  site(GPU, "handle static Tensor<GpuBuffer<float>>", "Tensor"),
];

/** The arm answering `op` in each handler: onLists in cpu.osp, then onGpu in gpu.osp. */
const tensorArms = (op: string): Site[] =>
  [CPU, GPU].map((file) => site(file, `\n            ${op} `, op));

const tensorDeclaration = (op: string): Site =>
  site(BACKENDS, `\n        ${op}: fn(`, op);

const tensorPerforms = (op: string, files: string[]): Site[] =>
  files.map((file) => site(file, `perform Backends::Tensor.${op}(`, op));

const tensorOperationCases = (): Case[] =>
  Object.entries(TENSOR_PERFORMS).flatMap(([op, files]) =>
    [tensorDeclaration(op), ...tensorPerforms(op, files)].map((from) => ({
      from,
      expect: tensorArms(op),
    })),
  );

const tensorEffectCases = (): Case[] =>
  [
    site(BACKENDS, "export static effect Tensor<B>", "Tensor"),
    ...TENSOR_HEADS,
    ...[MODEL, GENERATE, TRAIN].map((file) =>
      site(file, "perform Backends::Tensor.", "Tensor"),
    ),
  ].map((from) => ({ from, expect: TENSOR_HEADS }));

// --- gpt2: the `Fault` control effect and the builtin `Arith` ---------------

const FAULT_HEAD = site(BACKENDS, "handle Fault {", "Fault");
const HALT_ARM = site(BACKENDS, "halt fault =>", "halt");
const ARITH_HEAD = site(BACKENDS, "handle Arith {", "Arith");

const faultCases = (): Case[] => [
  { from: site(BACKENDS, "effect Fault {", "Fault"), expect: [FAULT_HEAD] },
  { from: FAULT_HEAD, expect: [FAULT_HEAD] },
  { from: site(BACKENDS, "control halt: fn", "halt"), expect: [HALT_ARM] },
  ...occurrences(3).flatMap((at) => [
    {
      from: site(BACKENDS, "perform Fault.halt(", "Fault", at),
      expect: [FAULT_HEAD],
    },
    {
      from: site(BACKENDS, "perform Fault.halt(", "halt", at),
      expect: [HALT_ARM],
    },
  ]),
  { from: ARITH_HEAD, expect: [ARITH_HEAD] },
];

// --- modules (ML): `Store`, `Audit` and the signature-promised `MetricsFx` --

/** Each `Store` operation's arm in `handler Store`, and who performs it. */
const STORE: Record<string, { arm: string; performs: string[] }> = {
  open: { arm: "open name =>", performs: [ROUTES] },
  credit: { arm: "credit id cents note =>", performs: [ROUTES] },
  debit: { arm: "debit id cents note =>", performs: [ROUTES] },
  transfer: { arm: "transfer fromId toId cents note =>", performs: [ROUTES] },
  balance: { arm: "balance id =>", performs: [ROUTES, MAIN] },
  owner: { arm: "owner id =>", performs: [ROUTES, MAIN] },
  count: { arm: "count =>", performs: [ROUTES, MAIN] },
  activity: { arm: "activity =>", performs: [ROUTES] },
};
const STORE_HEAD = site(LEDGER, "handler Store", "Store");

const storeCases = (): Case[] => [
  ...Object.entries(STORE).flatMap(([op, { arm, performs }]) => {
    const expect = [site(LEDGER, arm, op)];
    return [
      { from: site(LEDGER, `\n        ${op} : `, op), expect },
      ...performs.map((file) => ({
        from: site(file, `perform Ledger::Store.${op}`, op),
        expect,
      })),
    ];
  }),
  ...[
    site(LEDGER, "export effect Store", "Store"),
    STORE_HEAD,
    site(ROUTES, "perform Ledger::Store.", "Store"),
    site(MAIN, "perform Ledger::Store.", "Store"),
  ].map((from) => ({ from, expect: [STORE_HEAD] })),
];

const AUDIT_HEAD = site(MAIN, "handler Api::Audit", "Api::Audit");
const LOG_ARM = site(MAIN, "log line =>", "log");

const auditCases = (): Case[] => [
  { from: site(ROUTES, "export effect Audit", "Audit"), expect: [AUDIT_HEAD] },
  { from: site(MAIN, "handler Api::Audit", "Audit"), expect: [AUDIT_HEAD] },
  { from: site(ROUTES, "log : string => Unit", "log"), expect: [LOG_ARM] },
  ...occurrences(7).flatMap((at) => [
    {
      from: site(ROUTES, "perform Audit.log", "Audit", at),
      expect: [AUDIT_HEAD],
    },
    { from: site(ROUTES, "perform Audit.log", "log", at), expect: [LOG_ARM] },
  ]),
];

const METRICS_HEAD = site(METRICS, "handle MetricsFx", "MetricsFx");
const METRICS_ARMS: Record<string, Site> = {
  hit: site(METRICS, "hit _path =>", "hit"),
  total: site(METRICS, "total => served", "total"),
};

/** Occurrence 0 is the `signature MetricsApi` contract, 1 the state module. */
const metricsCases = (): Case[] => [
  ...occurrences(2).map((at) => ({
    from: site(METRICS, "effect MetricsFx", "MetricsFx", at),
    expect: [METRICS_HEAD],
  })),
  { from: METRICS_HEAD, expect: [METRICS_HEAD] },
  {
    from: site(MAIN, "perform Metrics::MetricsFx.", "MetricsFx"),
    expect: [METRICS_HEAD],
  },
  ...Object.entries(METRICS_ARMS).flatMap(([op, arm]) => [
    ...occurrences(2).map((at) => ({
      from: site(METRICS, `\n        ${op} : `, op, at),
      expect: [arm],
    })),
    { from: site(MAIN, `perform Metrics::MetricsFx.${op}`, op), expect: [arm] },
  ]),
];

// --- words that name no effect and no operation ------------------------------

const NOTHING: Site[] = [
  site(GPU, "fn answered(result)", "answered"),
  site(BACKENDS, "export type Kernel", "Kernel"),
  site(CPU, "handle static Tensor<List<float>>", "List"),
  site(MODEL, "perform Backends::Tensor.get", "Backends"),
  site(ROUTES, "perform Ledger::Store.owner", "Ledger"),
  site(LEDGER, "export sqlite db", "sqlite"),
];

// --- Go to Definition: operations, parameters and arm parameters -------------

/** Every arm and `perform` of an operation lands on its declaration; `work()`
 * lands on its own handler's parameter, and an arm's body on the arm's. */
const definitionCases = (): Case[] => [
  ...Object.entries(TENSOR_PERFORMS).flatMap(([op, files]) =>
    [...tensorArms(op), ...tensorPerforms(op, files)].map((from) => ({
      from,
      expect: [tensorDeclaration(op)],
    })),
  ),
  ...[
    [GPU, "fn onGpu(work)"],
    [CPU, "fn onLists(work)"],
  ].map(([file, binding]) => ({
    from: site(file, "\n        work()", "work"),
    expect: [site(file, binding, "work")],
  })),
  {
    from: site(GPU, "gpuZipWith(a, b, f)", "f"),
    expect: [site(GPU, "zipWith a b f =>", "f")],
  },
];

// --- the runner --------------------------------------------------------------

function workspaceRoot(): string {
  const root = process.env.OSPREY_VSIX_TEST_ROOT;
  assert.ok(root, "Installed VSIX runner must provide its isolated root");
  return fs.realpathSync(path.join(root, "workspace"));
}

function copyExamples(workspace: string): void {
  const source = process.env.OSPREY_VSIX_SOURCE_EXTENSION;
  assert.ok(source, "Installed VSIX runner must name the source checkout");
  for (const [from, to] of COPIES) {
    fs.cpSync(
      path.join(source, "..", "examples", from),
      path.join(workspace, to),
      {
        recursive: true,
      },
    );
  }
}

async function documentOf(
  workspace: string,
  file: string,
): Promise<vscode.TextDocument> {
  return vscode.workspace.openTextDocument(path.join(workspace, file));
}

/** The range of `site.word` inside its context occurrence. */
function rangeAt(document: vscode.TextDocument, at: Site): vscode.Range {
  const context = rangeOf(document, at.context, at.occurrence);
  const offset = at.context.indexOf(at.word);
  assert.notStrictEqual(
    offset,
    -1,
    `${at.word} inside ${JSON.stringify(at.context)}`,
  );
  const start = document.offsetAt(context.start) + offset;
  return new vscode.Range(
    document.positionAt(start),
    document.positionAt(start + at.word.length),
  );
}

function describe(file: string, range: vscode.Range): string {
  const { start, end } = range;
  return `${file}:${start.line}:${start.character}-${end.line}:${end.character}`;
}

async function expected(workspace: string, sites: Site[]): Promise<string[]> {
  const described = await Promise.all(
    sites.map(async (at) =>
      describe(at.file, rangeAt(await documentOf(workspace, at.file), at)),
    ),
  );
  return described.sort();
}

type Provider =
  "vscode.executeImplementationProvider" | "vscode.executeDefinitionProvider";

async function locationsAt(
  provider: Provider,
  workspace: string,
  document: vscode.TextDocument,
  position: vscode.Position,
): Promise<string[]> {
  const found = await vscode.commands.executeCommand<
    (vscode.Location | vscode.LocationLink)[]
  >(provider, document.uri, position);
  return (found ?? [])
    .map((item) => {
      const [uri, range] =
        "targetUri" in item
          ? [item.targetUri, item.targetSelectionRange ?? item.targetRange]
          : [item.uri, item.range];
      return describe(
        path.relative(workspace, fs.realpathSync(uri.fsPath)),
        range,
      );
    })
    .sort();
}

function title(test: Case, answer = "implementation"): string {
  const { file, context, word, occurrence = 0 } = test.from;
  const where = `${file} ${JSON.stringify(context.trim())}#${occurrence}`;
  return `${word} at ${where} → ${test.expect.length} ${answer}(s)`;
}

/** Wait until `provider` answers `each.from` with exactly `each.expect`. */
async function assertAnswers(
  provider: Provider,
  workspace: string,
  each: Case,
  label: string,
): Promise<void> {
  const document = await documentOf(workspace, each.from.file);
  const position = rangeAt(document, each.from).start;
  const want = await expected(workspace, each.expect);
  assert.ok(want.length > 0, "a positive case expects locations");
  const got = await waitFor(
    () => locationsAt(provider, workspace, document, position),
    (found) => JSON.stringify(found) === JSON.stringify(want),
    `${label}; expected ${JSON.stringify(want)}`,
  );
  assert.deepStrictEqual(got, want);
}

suite("Installed VSIX: Go to Implementations on effects and operations", () => {
  let workspace = "";
  // A negative answer is only evidence once the server has answered a
  // positive query for the same file; before that, an empty list proves
  // nothing about the feature.
  const answered = new Set<string>();

  suiteSetup(() => {
    workspace = workspaceRoot();
    copyExamples(workspace);
  });
  suiteTeardown(closeEditors);

  const positive = [
    ...tensorOperationCases(),
    ...tensorEffectCases(),
    ...faultCases(),
    ...storeCases(),
    ...auditCases(),
    ...metricsCases(),
  ];
  for (const each of positive) {
    test(title(each), async () => {
      await assertAnswers(
        "vscode.executeImplementationProvider",
        workspace,
        each,
        title(each),
      );
      answered.add(each.from.file);
    });
  }

  for (const from of NOTHING) {
    test(title({ from, expect: [] }), async () => {
      assert.ok(
        answered.has(from.file),
        `${from.file} answered a positive query first`,
      );
      const document = await documentOf(workspace, from.file);
      const position = rangeAt(document, from).start;
      assert.deepStrictEqual(
        await locationsAt(
          "vscode.executeImplementationProvider",
          workspace,
          document,
          position,
        ),
        [],
      );
    });
  }

  test("the matrix covers every operation and handler form it names", () => {
    assert.strictEqual(Object.keys(TENSOR_PERFORMS).length, 10);
    assert.strictEqual(Object.keys(STORE).length, 8);
    assert.ok(positive.length >= 90, `${positive.length} positive cases`);
  });

  // Hover on gpu.osp's `zipWith` arm named `Tensor.zipWith`, but Go to
  // Definition found nothing there, nor on `work()`.
  for (const each of definitionCases()) {
    const label = title(each, "definition");
    test(label, () =>
      assertAnswers("vscode.executeDefinitionProvider", workspace, each, label),
    );
  }
});
