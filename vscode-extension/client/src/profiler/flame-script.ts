// The inline <script> for the flame-graph webview ([PROF-VSCODE-FLAME]).
// Kept as a template string so flame-html.ts stays a pure string builder; the
// script reads its data from the <script type="application/json"> blob (no
// eval, no external resources — CSP-safe). Interactions: wheel = zoom about
// the cursor, drag = pan, hover = tooltip + highlight, click = select +
// postMessage to open the source, double-click = zoom to frame, search dims
// non-matching frames, Esc clears the search, 0 resets zoom.
// NOTE: the only interpolations are the flame-script-helpers function sources
// (type-checked + unit-tested TS, embedded verbatim via toString); the rest
// is one literal with no backticks or dollar-brace sequences inside.

import { clampViewRange, fitLabelText } from "./flame-script-helpers";

export const FLAME_SCRIPT = `(function () {
  "use strict";
  ${fitLabelText.toString()}
  ${clampViewRange.toString()}
  var vscode = typeof acquireVsCodeApi === "function"
    ? acquireVsCodeApi()
    : { postMessage: function () {} };
  var data = JSON.parse(document.getElementById("flame-data").textContent);
  var model = data.model;
  var ROW_H = 18;
  var MIN_SPAN = 0.00001;
  var LABEL_PAD = 4;
  var DIM_ALPHA = 0.22;
  var canvas = document.getElementById("flame");
  var ctx = canvas.getContext("2d");
  var tooltip = document.getElementById("tooltip");
  var searchBox = document.getElementById("search");
  var matchLabel = document.getElementById("match-label");
  var fiberSelect = document.getElementById("fiber-select");
  var btnLeft = document.getElementById("btn-left");
  var btnTime = document.getElementById("btn-time");
  var measures = {};
  var labelCache = {};
  var rowsCache = {};
  var matchSet = null;
  var drag = null;
  var state = { view: "left", fiber: 0, x0: 0, x1: 1, selected: -1, hover: null };

  model.fibers.forEach(function (f, i) {
    if (f.totalWeight > model.fibers[state.fiber].totalWeight) { state.fiber = i; }
  });

  function fiber() { return model.fibers[state.fiber]; }
  function rects() { return state.view === "left" ? fiber().leftHeavy : fiber().timeOrder; }
  function depthCount() {
    var d = state.view === "left" ? fiber().maxDepthLeft : fiber().maxDepthTime;
    return Math.max(d, 3);
  }
  function span() { return state.x1 - state.x0; }

  function rowsByDepth() {
    var key = state.fiber + ":" + state.view;
    if (!rowsCache[key]) {
      var byDepth = [];
      rects().forEach(function (r) {
        (byDepth[r.depth] || (byDepth[r.depth] = [])).push(r);
      });
      rowsCache[key] = byDepth;
    }
    return rowsCache[key];
  }

  function cssVar(name, fallback) {
    var value = getComputedStyle(document.body).getPropertyValue(name).trim();
    return value || fallback;
  }

  function resize() {
    var dpr = window.devicePixelRatio || 1;
    var width = canvas.parentElement.clientWidth;
    var height = depthCount() * ROW_H + 2;
    canvas.style.width = width + "px";
    canvas.style.height = height + "px";
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    render();
  }

  function measure(text) {
    if (!(text in measures)) { measures[text] = ctx.measureText(text).width; }
    return measures[text];
  }

  function fitLabel(name, avail) {
    var key = name + "|" + Math.floor(avail / 8);
    if (!(key in labelCache)) { labelCache[key] = fitLabelText(name, avail, measure); }
    return labelCache[key];
  }

  function drawRect(r, width) {
    var px0 = ((r.x0 - state.x0) / span()) * width;
    var px1 = ((r.x1 - state.x0) / span()) * width;
    if (px1 < 0 || px0 > width || px1 - px0 < 0.3) { return; }
    var y = r.depth * ROW_H;
    var w = Math.max(px1 - px0 - 1, 0.4);
    ctx.globalAlpha = matchSet !== null && !matchSet.has(r.frameIdx) ? DIM_ALPHA : 1;
    ctx.fillStyle = model.frames[r.frameIdx].color;
    ctx.fillRect(px0, y, w, ROW_H - 1);
    if (state.selected === r.frameIdx || state.hover === r) {
      ctx.strokeStyle = cssVar("--vscode-focusBorder", "#3794ff");
      ctx.strokeRect(px0 + 0.5, y + 0.5, Math.max(w - 1, 0.5), ROW_H - 2);
    }
    var vis0 = Math.max(px0, 0);
    var avail = Math.min(px1 - 1, width) - vis0 - 2 * LABEL_PAD;
    if (avail > measure("\\u2026") + 2) {
      ctx.fillStyle = "rgba(15, 15, 15, 0.9)";
      ctx.fillText(fitLabel(model.frames[r.frameIdx].name, avail), vis0 + LABEL_PAD, y + ROW_H / 2);
    }
    ctx.globalAlpha = 1;
  }

  function render() {
    var width = canvas.clientWidth;
    ctx.fillStyle = cssVar("--vscode-editor-background", "#1e1e1e");
    ctx.fillRect(0, 0, width, canvas.clientHeight);
    ctx.font = "11px " + cssVar("--vscode-font-family", "sans-serif");
    ctx.textBaseline = "middle";
    rects().forEach(function (r) { drawRect(r, width); });
  }

  function rectAt(offsetX, offsetY) {
    var row = rowsByDepth()[Math.floor(offsetY / ROW_H)];
    if (!row) { return null; }
    var x = state.x0 + (offsetX / canvas.clientWidth) * span();
    var lo = 0;
    var hi = row.length - 1;
    while (lo <= hi) {
      var mid = (lo + hi) >> 1;
      if (row[mid].x1 < x) { lo = mid + 1; }
      else if (row[mid].x0 > x) { hi = mid - 1; }
      else { return row[mid]; }
    }
    return null;
  }

  function fmtMs(seconds) { return (seconds * 1000).toFixed(1) + "ms"; }
  function fmtPct(part) {
    var total = fiber().totalWeight;
    return (total > 0 ? ((100 * part) / total).toFixed(1) : "0.0") + "%";
  }

  function tooltipLine(className, text) {
    var el = document.createElement("div");
    el.className = className;
    el.textContent = text;
    tooltip.appendChild(el);
  }

  function showTooltip(r, clientX, clientY) {
    var frame = model.frames[r.frameIdx];
    var stats = fiber().stats;
    tooltip.innerHTML = "";
    tooltipLine("tt-name", frame.name);
    tooltipLine("tt-loc", frame.file ? frame.file + ":" + frame.line : "(runtime)");
    tooltipLine("tt-nums",
      "self " + fmtMs(stats.self[r.frameIdx]) + " (" + fmtPct(stats.self[r.frameIdx]) + ") \\u00b7 " +
      "total " + fmtMs(stats.total[r.frameIdx]) + " (" + fmtPct(stats.total[r.frameIdx]) + ") \\u00b7 " +
      stats.count[r.frameIdx] + " samples");
    tooltip.style.display = "block";
    var maxLeft = document.body.clientWidth - tooltip.offsetWidth - 8;
    tooltip.style.left = Math.min(clientX + 14, Math.max(maxLeft, 0)) + "px";
    tooltip.style.top = (clientY + 14) + "px";
  }

  function clampView() {
    var next = clampViewRange(state.x0, state.x1, MIN_SPAN);
    state.x0 = next.x0;
    state.x1 = next.x1;
  }

  function resetZoom() {
    state.x0 = 0;
    state.x1 = 1;
    render();
  }

  function selectFrame(frameIdx) {
    state.selected = frameIdx;
    var frame = model.frames[frameIdx];
    if (frame.file) { vscode.postMessage({ type: "select", file: frame.file, line: frame.line }); }
    render();
  }

  canvas.addEventListener("wheel", function (e) {
    e.preventDefault();
    var anchor = state.x0 + (e.offsetX / canvas.clientWidth) * span();
    var frac = (anchor - state.x0) / span();
    var next = Math.min(Math.max(span() * Math.pow(1.0018, e.deltaY), MIN_SPAN), 1);
    state.x0 = anchor - frac * next;
    state.x1 = state.x0 + next;
    clampView();
    render();
  }, { passive: false });

  canvas.addEventListener("mousedown", function (e) {
    drag = { startX: e.clientX, x0: state.x0, span: span(), moved: false };
  });

  window.addEventListener("mousemove", function (e) {
    if (!drag) { return; }
    var dx = e.clientX - drag.startX;
    if (Math.abs(dx) > 3) { drag.moved = true; }
    state.x0 = drag.x0 - (dx / canvas.clientWidth) * drag.span;
    state.x1 = state.x0 + drag.span;
    clampView();
    render();
  });

  window.addEventListener("mouseup", function (e) {
    if (!drag) { return; }
    var wasDrag = drag.moved;
    drag = null;
    if (wasDrag || e.target !== canvas) { return; }
    var r = rectAt(e.offsetX, e.offsetY);
    if (r) { selectFrame(r.frameIdx); }
  });

  canvas.addEventListener("mousemove", function (e) {
    if (drag) { return; }
    var r = rectAt(e.offsetX, e.offsetY);
    if (r !== state.hover) {
      state.hover = r;
      render();
    }
    if (r) {
      showTooltip(r, e.clientX, e.clientY);
      canvas.style.cursor = "pointer";
    } else {
      tooltip.style.display = "none";
      canvas.style.cursor = "default";
    }
  });

  canvas.addEventListener("mouseleave", function () {
    state.hover = null;
    tooltip.style.display = "none";
    render();
  });

  canvas.addEventListener("dblclick", function (e) {
    var r = rectAt(e.offsetX, e.offsetY);
    if (!r) { return; }
    state.x0 = r.x0;
    state.x1 = Math.max(r.x1, r.x0 + MIN_SPAN);
    clampView();
    render();
  });

  // The matched share of the profile: matched slabs whose ancestors did not
  // already match, so overlapping frames never double-count (speedscope-style).
  function matchedTotalPct() {
    var counted = [];
    var sum = 0;
    rowsByDepth().forEach(function (row) {
      row.forEach(function (r) {
        if (!matchSet.has(r.frameIdx)) { return; }
        var mid = (r.x0 + r.x1) / 2;
        var inside = counted.some(function (c) { return mid >= c[0] && mid <= c[1]; });
        if (!inside) {
          counted.push([r.x0, r.x1]);
          sum += r.x1 - r.x0;
        }
      });
    });
    return 100 * sum;
  }

  function applySearch(query) {
    var needle = query.trim().toLowerCase();
    if (needle === "") {
      matchSet = null;
      matchLabel.textContent = "";
      render();
      return;
    }
    matchSet = new Set();
    model.frames.forEach(function (f, i) {
      if (f.name.toLowerCase().indexOf(needle) !== -1) { matchSet.add(i); }
    });
    matchLabel.textContent =
      matchSet.size + " frames \\u00b7 " + matchedTotalPct().toFixed(1) + "% of total";
    render();
  }

  searchBox.addEventListener("input", function () { applySearch(searchBox.value); });

  window.addEventListener("keydown", function (e) {
    if (e.key === "Escape") {
      searchBox.value = "";
      applySearch("");
    } else if (e.key === "0" && e.target !== searchBox) {
      resetZoom();
    }
  });

  function setView(view) {
    state.view = view;
    state.hover = null;
    btnLeft.classList.toggle("active", view === "left");
    btnTime.classList.toggle("active", view === "time");
    resize();
  }
  btnLeft.addEventListener("click", function () { setView("left"); });
  btnTime.addEventListener("click", function () { setView("time"); });

  function setFiber(index) {
    if (index < 0 || index >= model.fibers.length) { return; }
    state.fiber = index;
    state.hover = null;
    fiberSelect.value = String(index);
    document.querySelectorAll(".chip").forEach(function (chip) {
      chip.classList.toggle("active", chip.getAttribute("data-fiber") === String(index));
    });
    state.x0 = 0;
    state.x1 = 1;
    applySearch(searchBox.value);
    resize();
  }
  fiberSelect.addEventListener("change", function () { setFiber(Number(fiberSelect.value)); });
  document.querySelectorAll(".chip").forEach(function (chip) {
    chip.addEventListener("click", function () { setFiber(Number(chip.getAttribute("data-fiber"))); });
  });
  document.getElementById("btn-reset").addEventListener("click", resetZoom);

  document.querySelectorAll("tr[data-file]").forEach(function (row) {
    row.addEventListener("click", function () {
      vscode.postMessage({
        type: "select",
        file: row.getAttribute("data-file"),
        line: Number(row.getAttribute("data-line")),
      });
    });
  });

  window.addEventListener("resize", resize);
  setFiber(state.fiber);
})();
`;
