import * as assert from "node:assert/strict";
import { themeStyle } from "../../client/src/osprey-ui/theme";

suite("native application theme", () => {
  test("the application owns surfaces, typography colours and source roles", () => {
    const css = themeStyle({ tokens: [
      { name: "canvas", light: "#FAFAF7", dark: "#202723" },
      { name: "primary", light: "#242923", dark: "#ECEDE5" },
      { name: "sourceKeyword", light: "#794A64", dark: "#D5B0C7" },
      { name: "dangerSurface", light: "#F6E8E3", dark: "#453330" },
    ], feedback: 140, settle: 240 });
    assert.ok(css.includes("--osp-canvas: light-dark(#FAFAF7, #202723);"));
    assert.ok(css.includes("--osp-text: light-dark(#242923, #ECEDE5);"));
    assert.ok(css.includes("--osp-source-keyword: light-dark(#794A64, #D5B0C7);"));
    assert.ok(css.includes("--osp-danger-surface: light-dark(#F6E8E3, #453330);"));
    assert.ok(css.includes("--osp-motion:140ms;") && css.includes("--osp-settle:240ms;"));
  });

  test("malformed names, colours and motion cannot become style declarations", () => {
    const css = themeStyle({ tokens: [
      { name: "canvas;}body{color:red", light: "#FFFFFF", dark: "#000000" },
      { name: "canvas", light: "url(https://example.test)", dark: "#000000" },
      { name: "sourceKeyword", light: "#794A64", dark: "#D5B0C7" },
    ], feedback: "0;}body{color:red", settle: -1 });
    assert.ok(!css.includes("body") && !css.includes("url("));
    assert.ok(!css.includes("--osp-motion:") && !css.includes("--osp-settle:"));
    assert.ok(css.includes("--osp-source-keyword:"));
    assert.equal(themeStyle(null), "");
  });

  test("brand properties remain available and empty themes retain host defaults", () => {
    assert.ok(themeStyle({ tokens: [{ name: "accent", light: "#006B86", dark: "#77D7F4" }] })
      .includes("--osp-brand-accent: light-dark(#006B86, #77D7F4);"));
    assert.equal(themeStyle({ tokens: [] }), "");
  });
});
