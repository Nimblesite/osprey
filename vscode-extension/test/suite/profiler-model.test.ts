import * as assert from "assert";
import {
  buildFlameModel,
  colorForRank,
  frameColors,
  frameStats,
  isOspreySource,
  leftHeavyRects,
  parseSpeedscope,
  searchMatches,
  timeOrderRects,
  triangle,
  type RenderRect,
  type SpeedscopeFile,
  type SpeedscopeProfile,
} from "../../client/src/profiler/flame-model";

const FRAMES = [
  { name: "main", file: "/w/fib.osp", line: 10 },
  { name: "fib", file: "/w/fib.osp", line: 3 },
  { name: "memcpy", file: "/usr/lib/libc.dylib", line: 0 },
  { name: "helper", file: "/w/util.ospml", line: 2 },
];

// Root-first stacks; weights sum to 16 so config-space fractions are exact
// binary doubles and deepStrictEqual comparisons never hit rounding noise.
const PROFILE: SpeedscopeProfile = {
  type: "sampled",
  name: "main",
  unit: "seconds",
  startValue: 0,
  endValue: 16,
  samples: [[0], [0, 1], [0, 1], [0, 1, 2], [0, 3], [3]],
  weights: [2, 3, 4, 2, 4, 1],
};

const DOC: SpeedscopeFile = { shared: { frames: FRAMES }, profiles: [PROFILE] };

const byDepth = (rects: RenderRect[], depth: number): RenderRect[] =>
  rects.filter((r) => r.depth === depth);

suite("flame-model parseSpeedscope", () => {
  test("accepts a valid sampled document", () => {
    const parsed = parseSpeedscope(JSON.stringify(DOC));
    assert.ok(parsed.ok);
    assert.strictEqual(parsed.value.profiles[0].samples.length, 6);
  });

  test("rejects invalid JSON, missing frames, and missing profiles", () => {
    for (const bad of ["{nope", "{}", '{"shared":{"frames":[{"name":1}]},"profiles":[]}']) {
      const parsed = parseSpeedscope(bad);
      assert.ok(!parsed.ok, `expected rejection for ${bad}`);
    }
    const noProfiles = parseSpeedscope('{"shared":{"frames":[]},"profiles":[]}');
    assert.ok(!noProfiles.ok && noProfiles.error.includes("no profiles"));
  });

  test("rejects wrong profile type, length mismatch, and bad frame indices", () => {
    const evented = { ...PROFILE, type: "evented" };
    const mismatch = { ...PROFILE, weights: [1] };
    const badIndex = { ...PROFILE, samples: [[99]], weights: [1] };
    const noSamples = { ...PROFILE, samples: undefined as unknown as number[][] };
    for (const [profile, needle] of [
      [evented, "unsupported type"],
      [mismatch, "weights"],
      [badIndex, "outside shared.frames"],
      [noSamples, "missing samples"],
    ] as const) {
      const parsed = parseSpeedscope(JSON.stringify({ shared: { frames: FRAMES }, profiles: [profile] }));
      assert.ok(!parsed.ok && parsed.error.includes(needle), `expected "${needle}"`);
    }
  });
});

suite("flame-model left-heavy layout", () => {
  const rects = leftHeavyRects(PROFILE);

  test("merges samples and sorts siblings heaviest-first", () => {
    const roots = byDepth(rects, 0);
    assert.deepStrictEqual(roots.map((r) => [r.frameIdx, r.x0, r.x1, r.selfWeight, r.totalWeight]), [
      [0, 0, 0.9375, 2, 15],
      [3, 0.9375, 1, 1, 1],
    ]);
  });

  test("children accumulate weight and nest under the merged parent", () => {
    const depth1 = byDepth(rects, 1);
    assert.deepStrictEqual(depth1.map((r) => [r.frameIdx, r.x0, r.x1, r.selfWeight, r.totalWeight]), [
      [1, 0, 0.5625, 7, 9],
      [3, 0.5625, 0.8125, 4, 4],
    ]);
    assert.deepStrictEqual(byDepth(rects, 2).map((r) => [r.frameIdx, r.x0, r.x1]), [[2, 0, 0.125]]);
  });

  test("equal-weight siblings tie-break by frame index (deterministic)", () => {
    const tied: SpeedscopeProfile = { ...PROFILE, samples: [[1], [0]], weights: [1, 1] };
    assert.deepStrictEqual(byDepth(leftHeavyRects(tied), 0).map((r) => r.frameIdx), [0, 1]);
  });

  test("zero total weight yields no rects", () => {
    assert.deepStrictEqual(leftHeavyRects({ ...PROFILE, samples: [], weights: [] }), []);
  });
});

suite("flame-model time-order layout", () => {
  const rects = timeOrderRects(PROFILE);

  test("adjacent identical stacks merge into continuous slabs", () => {
    assert.deepStrictEqual(byDepth(rects, 1).map((r) => [r.frameIdx, r.x0, r.x1, r.selfWeight, r.totalWeight]), [
      [1, 0.125, 0.6875, 7, 9],
      [3, 0.6875, 0.9375, 4, 4],
    ]);
  });

  test("roots split where the bottom frame changes", () => {
    assert.deepStrictEqual(byDepth(rects, 0).map((r) => [r.frameIdx, r.x0, r.x1, r.selfWeight, r.totalWeight]), [
      [0, 0, 0.9375, 2, 15],
      [3, 0.9375, 1, 1, 1],
    ]);
    assert.deepStrictEqual(byDepth(rects, 2).map((r) => [r.frameIdx, r.x0, r.x1]), [[2, 0.5625, 0.6875]]);
  });

  test("empty profiles produce no rects", () => {
    assert.deepStrictEqual(timeOrderRects({ ...PROFILE, samples: [], weights: [] }), []);
  });
});

suite("flame-model frame stats", () => {
  test("total dedupes recursion; self goes to the leaf; counts are per sample", () => {
    const stats = frameStats(PROFILE, FRAMES.length);
    assert.deepStrictEqual(stats.total, [15, 9, 2, 5]);
    assert.deepStrictEqual(stats.self, [2, 7, 2, 5]);
    assert.deepStrictEqual(stats.count, [5, 3, 1, 2]);
  });

  test("a recursive stack counts its frame once", () => {
    const recursive: SpeedscopeProfile = { ...PROFILE, samples: [[1, 1]], weights: [2] };
    const stats = frameStats(recursive, FRAMES.length);
    assert.strictEqual(stats.total[1], 2);
    assert.strictEqual(stats.count[1], 1);
    assert.strictEqual(stats.self[1], 2);
  });

  test("an empty stack contributes nothing", () => {
    const stats = frameStats({ ...PROFILE, samples: [[]], weights: [5] }, FRAMES.length);
    assert.deepStrictEqual(stats.total, [0, 0, 0, 0]);
  });
});

suite("flame-model colors", () => {
  test("triangle wave hits its known points", () => {
    assert.strictEqual(triangle(0), 0);
    assert.strictEqual(triangle(1), 1);
    assert.strictEqual(triangle(2), 0);
    assert.strictEqual(triangle(0.5), 0.5);
    assert.strictEqual(triangle(3), 1);
  });

  test("osprey sources get the hue ramp; runtime frames a gray-blue", () => {
    assert.ok(isOspreySource("/w/a.osp") && isOspreySource("/w/a.ospml"));
    assert.ok(!isOspreySource("/usr/lib/libc.dylib") && !isOspreySource(""));
    assert.strictEqual(colorForRank("/usr/lib/libc.dylib", 0, 4), "hsl(215, 12%, 58.0%)");
    assert.ok(colorForRank("/w/a.osp", 1, 4).startsWith("hsl(80.0,"));
  });

  test("frame colors are deterministic and identity-stable under reordering", () => {
    const colors = frameColors(FRAMES);
    assert.deepStrictEqual(frameColors(FRAMES), colors);
    const reversed = [...FRAMES].reverse();
    const reversedColors = frameColors(reversed);
    FRAMES.forEach((frame, i) => {
      assert.strictEqual(reversedColors[reversed.indexOf(frame)], colors[i]);
    });
    assert.strictEqual(new Set(colors).size, colors.length, "all four frames distinct");
    assert.ok(colors[2].startsWith("hsl(215, 12%"), "runtime frame is gray-blue");
  });

  test("frames without a file rank as runtime", () => {
    assert.ok(frameColors([{ name: "anon" }])[0].startsWith("hsl(215"));
  });
});

suite("flame-model search", () => {
  test("case-insensitive substring over names", () => {
    assert.deepStrictEqual([...searchMatches(FRAMES, "FIB")], [1]);
    assert.deepStrictEqual([...searchMatches(FRAMES, "m")], [0, 2]);
    assert.deepStrictEqual([...searchMatches(FRAMES, "nomatch")], []);
  });

  test("blank and whitespace queries match nothing", () => {
    assert.strictEqual(searchMatches(FRAMES, "").size, 0);
    assert.strictEqual(searchMatches(FRAMES, "   ").size, 0);
  });
});

suite("flame-model buildFlameModel", () => {
  const model = buildFlameModel(DOC);

  test("assembles frames with colors and defaults for missing file/line", () => {
    assert.strictEqual(model.frames.length, 4);
    assert.strictEqual(model.frames[1].name, "fib");
    assert.strictEqual(model.frames[1].line, 3);
    const anon = buildFlameModel({
      shared: { frames: [{ name: "anon" }] },
      profiles: [{ ...PROFILE, samples: [[0]], weights: [1] }],
    });
    assert.strictEqual(anon.frames[0].file, "");
    assert.strictEqual(anon.frames[0].line, 0);
  });

  test("fibers carry both layouts, depths, totals, and stats", () => {
    const fiber = model.fibers[0];
    assert.strictEqual(fiber.name, "main");
    assert.strictEqual(fiber.totalWeight, 16);
    assert.strictEqual(fiber.sampleCount, 6);
    assert.strictEqual(fiber.maxDepthLeft, 3);
    assert.strictEqual(fiber.maxDepthTime, 3);
    assert.strictEqual(fiber.leftHeavy.length, 5);
    assert.strictEqual(fiber.timeOrder.length, 5);
    assert.deepStrictEqual(fiber.stats.total, [15, 9, 2, 5]);
  });
});
