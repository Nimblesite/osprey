import * as assert from "node:assert/strict";
import { renderWorkspace } from "../../client/src/osprey-ui/render";
import { workspaceHtml } from "../../client/src/osprey-ui/webview-html";

const image = { kind: "image", source: "media/claim.jpg", alt: "A claim and its assumptions", width: 1200, height: 800 };
const video = { kind: "video", source: "media/proof.mp4", poster: "media/poster.jpg", captions: "media/proof.vtt", alt: "A proof develops one justified step at a time", seconds: 40 };
const regions = (node: unknown) => new Map([["root", { revision: 1, node }]]);
const resolveAsset = (source: string) => `https://assets.example.test/${source}`;

suite("native lesson media", () => {
  test("images retain alternative text and intrinsic proportions without creating an action", () => {
    const html = renderWorkspace({ regions: regions(image), resolveAsset });
    assert.ok(html.includes('alt="A claim and its assumptions"'));
    assert.ok(html.includes('width="1200" height="800"'));
    assert.ok(html.includes('src="https://assets.example.test/media/claim.jpg"'));
    assert.ok(!html.includes("data-event-ordinal"));
  });

  test("videos are user-started with a poster and captions enabled by default", () => {
    const html = renderWorkspace({ regions: regions(video), resolveAsset });
    assert.ok(html.includes("<video") && html.includes(" controls") && html.includes(" playsinline"));
    assert.ok(!html.includes(" autoplay") && !html.includes(" loop"));
    assert.ok(html.includes('poster="https://assets.example.test/media/poster.jpg"'));
    assert.ok(html.includes('<track kind="captions"') && html.includes('src="https://assets.example.test/media/proof.vtt" default'));
    assert.ok(html.includes('aria-label="A proof develops one justified step at a time"'));
    assert.ok(!html.includes("data-event-ordinal"));
  });

  test("media text and resolved URLs cannot inject markup", () => {
    const html = renderWorkspace({ regions: regions({ ...image, alt: '"><script>bad</script>' }),
      resolveAsset: () => 'https://assets.example.test/a.jpg" onerror="bad' });
    assert.ok(!html.includes("<script>") && !html.includes(' onerror="bad'));
    assert.ok(html.includes("&lt;script&gt;bad&lt;/script&gt;"));
  });

  test("missing assets and malformed media fail explicitly", () => {
    assert.throws(() => renderWorkspace({ regions: regions(image) }), /asset/i);
    for (const node of [{ ...image, alt: "" }, { ...image, width: -1 }, { ...video, seconds: 0 }, { ...video, captions: "" }]) {
      assert.throws(() => renderWorkspace({ regions: regions(node), resolveAsset }), /media|asset/i);
    }
    assert.throws(() => renderWorkspace({ regions: regions(image), resolveAsset: () => "javascript:alert(1)" }), /asset/i);
  });

  test("the document permits approved local media without enabling remote connections", () => {
    const html = workspaceHtml({ nonce: "test", assetUri: "", cspSource: "vscode-resource:" });
    assert.ok(html.includes("media-src vscode-resource: data: blob:"));
    assert.ok(html.includes("connect-src 'none'"));
  });
});
