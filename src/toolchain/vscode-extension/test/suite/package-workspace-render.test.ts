// [NATIVE-SHM-VIEW] [GPUI-HOST-RENDER]: the webview draws a checked frame as DOM. Every kind the
// application can publish is drawn, every style value comes from the row, and the document that
// loads the webview grants it only this extension's own script.
import * as assert from "assert";
import { renderShown } from "../../client/src/osprey-ui/webview/render";
import { readFrame } from "../../client/src/osprey-ui/webview/rows";
import { workspaceHtml } from "../../client/src/osprey-ui/webview-html";
import { frame, type ControlSpec } from "./native-rows-kit";

function page(views: readonly ControlSpec[]): string {
  return renderShown(readFrame(frame({ views })));
}

suite("native row rendering", () => {
  test("every published control kind is drawn, and a choice draws nothing", () => {
    const kinds = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
    for (const kind of kinds) {
      const drawn = page([{ kind, action: 1, words: "words", hint: "search", width: 10, height: 10 }]);
      if (kind === 5) assert.ok(drawn.includes("flex-grow:1"), "a gap draws its flexible space");
      else if (kind === 10) assert.ok(drawn.includes("osp-rows"), "a choice draws no element of its own");
      else if (kind === 13) assert.ok(drawn.includes('<canvas class="osp-canvas"'), `a canvas is drawn: ${drawn}`);
      else assert.ok(drawn.includes("osp-control"), `kind ${kind} is drawn: ${drawn}`);
    }
    assert.ok(!page([{ kind: 10, words: "hidden" }]).includes("hidden"), "a choice never leaks its words");
  });

  test("a nested page keeps drawing order, parents and fixed extents", () => {
    const drawn = page([
      { kind: 0, words: "Page", fill: 0x101010ff, padding: 20 },
      { kind: 2, parent: 0, words: "Title", size: 32, weight: 800 },
      { kind: 1, parent: 0, spacing: 10, grow: 1 },
      { kind: 3, parent: 2, action: 0, words: "One", width: 100, height: 40 },
      { kind: 3, parent: 2, action: 1, words: "Two" },
    ]);
    assert.ok(drawn.indexOf("Title") < drawn.indexOf("One"), drawn);
    assert.ok(drawn.indexOf("One") < drawn.indexOf("Two"), drawn);
    assert.strictEqual((drawn.match(/data-ordinal="[01]" data-words=""/g) ?? []).length, 2);
    assert.ok(drawn.includes("width:100px"), drawn);
    assert.ok(drawn.includes("height:40px"), drawn);
    assert.ok(drawn.includes("flex-grow:1"), drawn);
    assert.ok(drawn.includes("background:#101010ff"), drawn);
  });

  test("image rows use only host-approved asset URIs", () => {
    const controls: readonly ControlSpec[] = [{ kind: 12, words: "media/logo.png", width: 40 }];
    const missing = renderShown(readFrame(frame({ views: controls })));
    assert.ok(missing.includes("osp-image-missing"), missing);
    assert.ok(!missing.includes('src="media/logo.png"'), "an unresolved path is never a src");
    const approved = renderShown(readFrame(frame({ views: controls })), { assets: new Map([["media/logo.png", "vscode-resource://approved/logo.png"]]) });
    assert.ok(approved.includes('src="vscode-resource://approved/logo.png"'), approved);
    assert.ok(!approved.includes("osp-image-missing"), approved);
  });

  test("a canvas draws a scene surface with its control number and focus identity", () => {
    const drawn = renderShown(readFrame(frame({ views: [{ kind: 13, action: 3 }],
      scenes: [{ code: 20, index: 0, x: 400, y: 300, width: 1 }, { code: 0, x: 100, y: 100, width: 200, height: 100, colour: 0x336699ff }] })));
    assert.ok(drawn.includes('<canvas class="osp-canvas"'), drawn);
    assert.ok(drawn.includes('data-scene="0"'), drawn);
    assert.ok(drawn.includes('data-ordinal="3"'), drawn);
    assert.ok(drawn.includes("min-height:320px"), drawn);
    assert.ok(drawn.includes('role="img"'), drawn);
  });

  test("a sheet leads with a dismissal that delivers its control number", () => {
    const drawn = page([{ kind: 16, action: 8, words: "Details" }, { kind: 2, parent: 0, words: "Body" }]);
    assert.ok(drawn.includes('data-ordinal="8" data-words=""'), drawn);
    assert.ok(drawn.includes('aria-label="Dismiss"'), drawn);
    assert.ok(drawn.indexOf("Dismiss") < drawn.indexOf("Body"), drawn);
  });

  test("the webview document loads only this extension's script and escapes its values", () => {
    const html = workspaceHtml({ nonce: "n0nce", assetUri: "vscode-resource://icon.png",
      scriptUri: "vscode-resource://out/webview/webview/main.js?x=<script>", cspSource: "vscode-resource:",
      loadingText: "<script>alert(1)</script>" });
    assert.ok(html.includes("script-src vscode-resource: 'nonce-n0nce'"), html);
    assert.ok(html.includes('<script type="module" nonce="n0nce"'), html);
    assert.ok(html.includes("out/webview/webview/main.js?x=&lt;script&gt;"), html);
    assert.ok(!html.includes("<script>alert(1)</script>"), html);
    assert.ok(html.includes("&lt;script&gt;alert(1)&lt;/script&gt;"), html);
    assert.ok(html.includes("connect-src 'none'"), html);
    assert.ok(html.includes("default-src 'none'"), html);
  });

  test("a refused frame leaves the last good drawing untouched", () => {
    const good = readFrame(frame({ views: [{ kind: 0 }, { kind: 2, parent: 0, words: "Kept" }] }));
    const refused = (): string => renderShown(readFrame({ shapes: new ArrayBuffer(0), views: new ArrayBuffer(7), text: new ArrayBuffer(0) }));
    assert.ok(renderShown(good).includes("Kept"));
    assert.throws(refused, /whole number of rows/);
  });
});
