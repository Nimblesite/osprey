import * as assert from "assert";
import { buildFlameHtml, escapeHtml, makeNonce, pathBasename } from "../../client/src/profiler/flame-html";
import { buildFlameModel, type SpeedscopeFile } from "../../client/src/profiler/flame-model";
import { FLAME_SCRIPT } from "../../client/src/profiler/flame-script";
import { clampViewRange, fitLabelText } from "../../client/src/profiler/flame-script-helpers";
import type { ProfileSummary } from "../../client/src/profiler/summary";

const DOC: SpeedscopeFile = {
  shared: {
    frames: [
      { name: "main", file: "/w/fib.osp", line: 10 },
      { name: "evil</script>", file: "", line: 0 },
    ],
  },
  profiles: [
    {
      type: "sampled",
      name: "main",
      unit: "seconds",
      startValue: 0,
      endValue: 1,
      samples: [[0], [0, 1]],
      weights: [0.5, 0.5],
    },
  ],
};

const SUMMARY: ProfileSummary = {
  version: 1,
  program: "/w/fib.osp",
  wallSeconds: 4.2,
  cpuSeconds: 3.9,
  sampleCount: 4182,
  rateHz: 997,
  droppedSamples: 0,
  fibers: [
    { id: 0, label: "main", samples: 100, oncpuSamples: 90 },
    { id: 1, label: "fiber-1", samples: 50, oncpuSamples: 25 },
    { id: 2, label: "fiber-2", samples: 0, oncpuSamples: 0 },
  ],
  hotFunctions: [
    { name: "fib", file: "/w/fib.osp", line: 3, selfPct: 42, totalPct: 61, selfSamples: 420, totalSamples: 610, kind: "user" },
    { name: "memcpy", file: "", line: 0, selfPct: 5, totalPct: 5.5, selfSamples: 50, totalSamples: 55, kind: "runtime" },
  ],
  hotLines: [{ file: "/w/fib.osp", line: 5, pct: 12, samples: 120 }],
};

const NONCE = "TESTNONCE123";
const html = buildFlameHtml({
  nonce: NONCE,
  title: 'fib<&>"osp',
  model: buildFlameModel(DOC),
  summary: SUMMARY,
});

suite("flame-html document structure", () => {
  test("is a complete themed document with canvas, tooltip, and toolbar", () => {
    assert.ok(html.startsWith("<!DOCTYPE html>"));
    assert.ok(html.includes('<canvas id="flame"></canvas>'));
    assert.ok(html.includes('<div id="tooltip"></div>'));
    assert.ok(html.includes(">Left Heavy</button>"));
    assert.ok(html.includes(">Time Order</button>"));
    assert.ok(html.includes('id="btn-reset"'));
    assert.ok(html.includes('id="search"'));
    assert.ok(html.includes("--vscode-editor-background"));
  });

  test("CSP allows only nonce'd inline style and script", () => {
    assert.ok(
      html.includes(
        `content="default-src 'none'; style-src 'nonce-${NONCE}'; script-src 'nonce-${NONCE}';"`,
      ),
    );
    assert.ok(html.includes(`<style nonce="${NONCE}">`));
    assert.ok(html.includes(`<script nonce="${NONCE}">`));
    assert.ok(!html.includes("http://") && !html.includes("https://"), "no external resources");
  });

  test("embeds the model as a JSON blob with < escaped", () => {
    assert.ok(html.includes(`<script type="application/json" id="flame-data" nonce="${NONCE}">`));
    assert.ok(html.includes('"fibers"') && html.includes('"leftHeavy"'));
    assert.ok(html.includes("evil\\u003c/script>"), "frame names cannot close the data blob");
  });

  test("escapes the title and includes the inline renderer", () => {
    assert.ok(html.includes("<title>Profile: fib&lt;&amp;&gt;&quot;osp</title>"));
    assert.ok(html.includes("acquireVsCodeApi"));
  });
});

suite("flame-html header and fiber controls", () => {
  test("header strip formats the run summary", () => {
    assert.ok(html.includes("4182 samples · 997Hz · 4.2s wall · 3.9s CPU · 3 fibers"));
  });

  test("per-fiber chips show on-CPU share; unsampled fibers get no chip", () => {
    assert.ok(html.includes('<button class="chip" data-fiber="0">main · 90% on-CPU</button>'));
    assert.ok(html.includes('data-fiber="1">fiber-1 · 50% on-CPU'));
    // fiber-2 still appears in the embedded JSON summary, but gets no chip.
    assert.ok(!html.includes("fiber-2 · 0% on-CPU"), "zero-sample fibers must not render chips");
    assert.ok(!html.includes('data-fiber="2"'));
  });

  test("a zero-sample fiber row between sampled ones does not shift chip indices", () => {
    // The model only builds fibers that produced samples ("alpha", "beta");
    // the summary interleaves an idle zero-sample row. Chip indices must stay
    // aligned with model.fibers, or clicking "beta" selects the wrong fiber.
    const profile = DOC.profiles[0];
    const model = buildFlameModel({
      shared: DOC.shared,
      profiles: [{ ...profile, name: "alpha" }, { ...profile, name: "beta" }],
    });
    const withIdleRow = buildFlameHtml({
      nonce: NONCE,
      title: "idle",
      model,
      summary: {
        ...SUMMARY,
        fibers: [
          { id: 0, label: "alpha", samples: 80, oncpuSamples: 40 },
          { id: 1, label: "idle", samples: 0, oncpuSamples: 0 },
          { id: 2, label: "beta", samples: 20, oncpuSamples: 20 },
        ],
      },
    });
    assert.ok(withIdleRow.includes('data-fiber="0">alpha · 50% on-CPU'));
    assert.ok(withIdleRow.includes('data-fiber="1">beta · 100% on-CPU'));
    assert.ok(!withIdleRow.includes(">idle ·"), "no chip for the idle fiber");
    assert.strictEqual(model.fibers[1].name, "beta", "chip 1 selects model fiber 1 = beta");
  });

  test("the fiber select lists each speedscope profile by name", () => {
    assert.ok(html.includes('<select id="fiber-select"><option value="0">main</option></select>'));
  });
});

suite("flame-html hot functions table", () => {
  test("renders SELF%/TOTAL%/FUNCTION/LOCATION rows with navigation attrs", () => {
    assert.ok(html.includes("<th>FUNCTION</th><th>LOCATION</th>"));
    assert.ok(
      html.includes(
        '<tr data-file="/w/fib.osp" data-line="3"><td class="num">42.0</td><td class="num">61.0</td><td>fib</td><td class="loc">fib.osp:3</td></tr>',
      ),
    );
  });

  test("fileless runtime rows carry no navigation and show a dash", () => {
    assert.ok(html.includes("<tr><td class=\"num\">5.0</td><td class=\"num\">5.5</td><td>memcpy</td><td class=\"loc\">—</td></tr>"));
  });
});

suite("flame-html helpers", () => {
  test("escapeHtml covers all five entities", () => {
    assert.strictEqual(escapeHtml(`<a href="x">&'</a>`), "&lt;a href=&quot;x&quot;&gt;&amp;&#39;&lt;/a&gt;");
  });

  test("pathBasename handles slashes, backslashes, and edge cases", () => {
    assert.strictEqual(pathBasename("/a/b/c.osp"), "c.osp");
    assert.strictEqual(pathBasename("a\\b\\c.osp"), "c.osp");
    assert.strictEqual(pathBasename("plain"), "plain");
    assert.strictEqual(pathBasename("dir/"), "dir/");
  });

  test("makeNonce emits 32 alphanumerics and varies", () => {
    const nonce = makeNonce();
    assert.match(nonce, /^[A-Za-z0-9]{32}$/);
    assert.notStrictEqual(nonce, makeNonce());
  });
});

suite("flame-script interactions", () => {
  test("wires zoom, pan, hover, select, search, and reset", () => {
    for (const needle of [
      '"wheel"',
      '"mousedown"',
      '"dblclick"',
      '"keydown"',
      "Escape",
      "postMessage",
      "devicePixelRatio",
      "measureText",
      "flame-data",
      "acquireVsCodeApi",
    ]) {
      assert.ok(FLAME_SCRIPT.includes(needle), `script should include ${needle}`);
    }
  });

  test("FLAME_SCRIPT is syntactically valid JavaScript", () => {
    // The webview script ships as an embedded string; a typo would otherwise
    // only surface as a dead panel at runtime. new Function parses (but does
    // not execute) it, so any syntax error fails the suite here.
    assert.doesNotThrow(() => new Function(FLAME_SCRIPT));
  });

  test("composes the tested helper sources verbatim", () => {
    assert.ok(FLAME_SCRIPT.includes(fitLabelText.toString()));
    assert.ok(FLAME_SCRIPT.includes(clampViewRange.toString()));
  });
});

suite("flame-script pure helpers", () => {
  // A deterministic fake for canvas measureText: 10px per character.
  const measure = (text: string): number => 10 * text.length;

  test("fitLabelText keeps names that fit and middle-elides ones that don't", () => {
    assert.strictEqual(fitLabelText("main", 40, measure), "main");
    const fitted = fitLabelText("averyveryverylongfunctionname", 120, measure);
    assert.ok(fitted.includes("…"));
    assert.ok(measure(fitted) <= 120);
    assert.ok(fitted.startsWith("avery") && fitted.endsWith("name"));
  });

  test("fitLabelText degrades to a lone ellipsis when nothing fits", () => {
    assert.strictEqual(fitLabelText("abcdefgh", 5, measure), "…");
    assert.strictEqual(fitLabelText("ab", 5, measure), "…");
  });

  test("clampViewRange enforces span and [0,1] bounds", () => {
    assert.deepStrictEqual(clampViewRange(0.25, 0.75, 0.001), { x0: 0.25, x1: 0.75 });
    assert.deepStrictEqual(clampViewRange(-0.25, 0.25, 0.001), { x0: 0, x1: 0.5 });
    assert.deepStrictEqual(clampViewRange(0.75, 1.5, 0.001), { x0: 0.25, x1: 1 });
    assert.deepStrictEqual(clampViewRange(-1, 2, 0.001), { x0: 0, x1: 1 });
    assert.deepStrictEqual(clampViewRange(0.5, 0.5, 0.25), { x0: 0.5, x1: 0.75 });
  });
});
