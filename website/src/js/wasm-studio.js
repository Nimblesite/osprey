// /wasm/ page — an Osprey wasm module seeds a browser SQLite database; the user
// adds rows and writes queries against it. SQL and Osprey are both highlighted
// with Prism using the SAME Osprey grammar the site build uses (eleventy.config.mjs).
import { runModule } from "/wasm/wasi-shim.mjs";
// The site-wide Osprey grammar, so this page colours exactly like every other
// Osprey snippet (eleventy.config.mjs highlights those with the same module).
import { ospreyGrammar as OSPREY_GRAMMAR } from "/js/osprey-grammar.mjs";

const $ = (id) => document.getElementById(id);
const esc = (s) => String(s).replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" }[c]));

const OSP_WASM = "/wasm/build/studio.osp.wasm";
const OSP_SRC = "/wasm/studio.osp";
const SQLJS = "https://cdn.jsdelivr.net/npm/sql.js@1.11.0/dist/";
const PRISM = "https://cdn.jsdelivr.net/npm/prismjs@1.29.0/";

const PRESETS = [
  { label: "revenue by region", sql: "SELECT region, SUM(qty * price) AS revenue, SUM(qty) AS units\nFROM sales GROUP BY region ORDER BY revenue DESC;" },
  { label: "top products", sql: "SELECT product, SUM(qty) AS units, SUM(qty * price) AS revenue\nFROM sales GROUP BY product ORDER BY revenue DESC;" },
  { label: "biggest orders", sql: "SELECT id, product, region, qty, price, qty * price AS revenue\nFROM sales ORDER BY revenue DESC LIMIT 5;" },
  { label: "all rows", sql: "SELECT * FROM sales ORDER BY id;" },
];

const state = { SQL: null, db: null, ddl: "", seed: "" };

// ── deps loaded from CDN (Prism core + SQL grammar; sql.js) ─────────────────
function loadScript(src) {
  return new Promise((resolve, reject) => {
    if (document.querySelector(`script[src="${src}"]`)) return resolve();
    const s = document.createElement("script");
    s.src = src;
    s.onload = resolve;
    s.onerror = () => reject(new Error("could not load " + src));
    document.head.appendChild(s);
  });
}

async function initPrism() {
  await loadScript(PRISM + "components/prism-core.min.js");
  await loadScript(PRISM + "components/prism-sql.min.js");
  window.Prism.languages.osprey = OSPREY_GRAMMAR;
}

async function initSqlite() {
  await loadScript(SQLJS + "sql-wasm.js");
  return window.initSqlJs({ locateFile: (f) => SQLJS + f });
}

function hl(code, lang) {
  const P = window.Prism;
  return P && P.languages[lang] ? P.highlight(code, P.languages[lang], lang) : esc(code);
}

// ── run the Osprey wasm module, keep only the DDL + seed it emits ───────────
async function runOsprey() {
  const res = await fetch(OSP_WASM);
  if (!res.ok) throw new Error(`fetch ${OSP_WASM}: HTTP ${res.status}`);
  let text = "";
  await runModule(await res.arrayBuffer(), (t) => {
    text += t;
  });
  const grab = (start) => {
    const lines = [];
    let on = false;
    for (const line of text.split("\n")) {
      if (line === start) on = true;
      else if (line.startsWith("::")) on = false;
      else if (on) lines.push(line);
    }
    return lines.join("\n").trim();
  };
  state.ddl = grab("::DDL");
  state.seed = grab("::SEED");
}

function seedDb() {
  if (state.db) state.db.close();
  state.db = new state.SQL.Database();
  state.db.run(state.ddl);
  state.db.run(state.seed);
}

// ── rendering ───────────────────────────────────────────────────────────────
function tableHTML(result) {
  if (!result || !result.columns.length) return `<p class="muted">No rows.</p>`;
  const head = result.columns.map((c) => `<th>${esc(c)}</th>`).join("");
  const isNum = result.columns.map((_, i) => result.values.every((r) => typeof r[i] === "number"));
  const body = result.values
    .map((row) => "<tr>" + row.map((v, i) => `<td class="${isNum[i] ? "num" : ""}">${esc(v === null ? "null" : v)}</td>`).join("") + "</tr>")
    .join("");
  return `<table class="data"><thead><tr>${head}</tr></thead><tbody>${body}</tbody></table>`;
}

function renderPresets() {
  $("presets").innerHTML = PRESETS.map((p, i) => `<button class="preset" data-i="${i}">${esc(p.label)}</button>`).join("");
  $("presets")
    .querySelectorAll(".preset")
    .forEach((b) =>
      b.addEventListener("click", () => {
        $("sql").value = PRESETS[Number(b.dataset.i)].sql;
        syncEditor();
        runQuery();
      })
    );
}

// Keep the highlight layer under the transparent textarea in sync.
function syncEditor() {
  $("sql-hl").innerHTML = hl($("sql").value, "sql");
}

function runQuery() {
  const status = $("sql-status");
  const out = $("sql-result");
  const sql = $("sql").value.trim();
  if (!state.db || !sql) return;
  try {
    const t0 = performance.now();
    const result = state.db.exec(sql);
    const rows = result.length ? result[0].values.length : 0;
    out.innerHTML = result.length ? result.map(tableHTML).join("") : `<p class="muted">Statement ran — no result set.</p>`;
    out.hidden = false;
    status.className = "console-status ok";
    status.textContent = `${rows} row${rows === 1 ? "" : "s"} in ${(performance.now() - t0).toFixed(1)} ms`;
  } catch (err) {
    status.className = "console-status err";
    status.textContent = String(err.message || err);
  }
}

function nextId() {
  const r = state.db.exec("SELECT COALESCE(MAX(id), 0) + 1 FROM sales");
  return r.length ? r[0].values[0][0] : 1;
}

function addRow(event) {
  event.preventDefault();
  const status = $("add-status");
  const f = event.target;
  const product = f.product.value.trim();
  const region = f.region.value;
  const qty = Number(f.qty.value);
  const price = Number(f.price.value);
  if (!state.db || !product || qty < 1 || price < 1) return;
  try {
    state.db.run("INSERT INTO sales VALUES (?, ?, ?, ?, ?)", [nextId(), product, region, qty, price]);
    status.className = "add-status ok";
    status.textContent = `Added ${product} (${region}) — run a query to see it.`;
  } catch (err) {
    status.className = "add-status err";
    status.textContent = String(err.message || err);
  }
}

function setBanner(kind, html) {
  const b = $("banner");
  b.className = "banner" + (kind ? " " + kind : "");
  b.innerHTML = html;
}

// ── boot ─────────────────────────────────────────────────────────────────────
async function boot() {
  setBanner("", `<span class="spin"></span> Running the Osprey wasm module…`);
  try {
    await Promise.all([runOsprey(), initPrism()]);
  } catch (err) {
    setBanner("warn", `Could not run the Osprey module: <code>${esc(err.message || err)}</code>. Build assets with <code>make wasm-site</code>.`);
    return;
  }

  fetch(OSP_SRC)
    .then((r) => r.text())
    .then((src) => ($("src-code").innerHTML = hl(src, "osprey")))
    .catch(() => ($("src-code").textContent = "// could not load source"));

  renderPresets();
  syncEditor();

  setBanner("", `<span class="spin"></span> Loading SQLite (sql.js)…`);
  try {
    state.SQL = await initSqlite();
    seedDb();
    setBanner("ok", `Database ready — Osprey seeded ${state.seed.split("\n").length} rows. Add data or write a query.`);
    runQuery();
  } catch (err) {
    setBanner("warn", `SQLite could not load from the CDN: <code>${esc(err.message || err)}</code>.`);
  }
}

$("add-form").addEventListener("submit", addRow);
$("run-sql").addEventListener("click", runQuery);
$("reset-db").addEventListener("click", () => {
  seedDb();
  $("add-status").textContent = " ";
  const s = $("sql-status");
  s.className = "console-status ok";
  s.textContent = "database reset to Osprey seed";
});
const editor = $("sql");
editor.addEventListener("input", syncEditor);
editor.addEventListener("scroll", () => {
  const hlEl = $("sql-hl").parentElement;
  hlEl.scrollTop = editor.scrollTop;
  hlEl.scrollLeft = editor.scrollLeft;
});
editor.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key === "Enter") runQuery();
});

boot().catch((err) => setBanner("warn", `Unexpected error: <code>${esc(err.message || err)}</code>`));
