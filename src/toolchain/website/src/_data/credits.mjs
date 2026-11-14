// Build-time credits data. Eleventy calls this default-exported function and
// exposes the result as the global `credits`. Packages are read straight from
// src/osprey/packages/license-audit.json (the single source of truth for package
// provenance); the small `apps` list below covers the apps and examples that
// have no audit entry. No package facts are duplicated by hand here.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const REPO = "https://github.com/Nimblesite/osprey";
const AUDIT = fileURLToPath(
  new URL("../../../../osprey/packages/license-audit.json", import.meta.url)
);

// Authors, display project names and relationships, verified from the
// repository's own records (src/osprey/packages/README.md, each package's NOTICE.md,
// src/osprey/examples/python/ports/NOTICE.md, and the audit's notice_attribution). Where
// the records name a project rather than a person, "contributors of X" credits
// the project instead of guessing at names. These are the only hand-kept facts;
// licences, links and audit status all come from license-audit.json.
const PACKAGES = {
  chromecast: {
    project: "CPython CAST v2 sender (project-owned)",
    authors: "NIMBLESITE PTY LTD, Australia",
    relationship: "port",
  },
  einops: {
    project: "einops",
    authors: "Alex Rogozhnikov and the einops contributors",
    relationship: "port",
  },
  faiss: {
    project: "Faiss",
    authors: "Meta Platforms, Inc. and the Faiss contributors",
    relationship: "port",
  },
  fastapi: {
    project: "FastAPI",
    authors: "Sebastián Ramírez and the FastAPI contributors",
    relationship: "port",
  },
  gguf: {
    project: "GGUF",
    authors: "Georgi Gerganov and the ggml contributors",
    relationship: "port",
  },
  flash_attn: {
    project: "FlashAttention",
    authors: "contributors of flash-attention (Dao-AILab)",
    relationship: "port",
  },
  jinja2: {
    project: "Jinja",
    authors: "the Pallets team and the Jinja contributors",
    relationship: "port",
  },
  micrograd: {
    project: "micrograd",
    authors: "Andrej Karpathy",
    relationship: "port",
  },
  native: {
    project: "Osprey (native UI framework)",
    authors: "NIMBLESITE PTY LTD, Australia",
    relationship: "original",
  },
  numpy: {
    project: "NumPy",
    authors: "contributors of NumPy",
    relationship: "port",
  },
  pydantic_ai: {
    project: "pydantic-ai",
    authors:
      "Pydantic Services Inc. and the pydantic-ai contributors (Douwe Maan, David Sanchez, Aditya Vardhan, David Montague, Marcelo Trylesinski, Alex Hall, Samuel Colvin, and others)",
    licenseUrl: "https://github.com/pydantic/pydantic-ai/blob/v2.46.0/LICENSE",
    relationship: "port",
  },
  re: {
    project: "CPython re",
    authors: "contributors of CPython, and the Unicode Consortium (derived tables)",
    relationship: "port",
  },
  rouge_score: {
    project: "rouge-score",
    authors: "The Google Research Authors, and the NLTK contributors",
    relationship: "port",
  },
  sacrebleu: {
    project: "SacreBLEU",
    authors: "contributors of SacreBLEU",
    relationship: "port",
  },
  safetensors: {
    project: "safetensors",
    authors: "contributors of safetensors (Hugging Face)",
    relationship: "port",
  },
  scipy: {
    project: "SciPy (special functions)",
    authors:
      "the SciPy developers; retained numerical kernels by Stephen L. Moshier (Cephes) and John Maddock (Boost)",
    relationship: "port",
  },
  signal_flow: {
    project: "Osprey (signal-flow editor)",
    authors: "NIMBLESITE PTY LTD, Australia",
    relationship: "original",
  },
  synth: {
    project: "Osprey (synthesis)",
    authors: "NIMBLESITE PTY LTD, Australia",
    relationship: "original",
  },
  sklearn: {
    project: "scikit-learn",
    authors: "contributors of scikit-learn",
    relationship: "port",
  },
  testing: {
    project: "NumPy (closeness comparison)",
    authors: "contributors of NumPy",
    relationship: "inspired by",
  },
  tiktoken: {
    project: "tiktoken",
    authors: "OpenAI and the tiktoken contributors, and the Unicode Consortium (derived tables)",
    relationship: "port",
  },
  tokenizers: {
    project: "tokenizers",
    authors:
      "contributors of Hugging Face tokenizers, and the Unicode Consortium (derived tables)",
    relationship: "port",
  },
  torch: {
    project: "PyTorch",
    authors: "contributors of PyTorch",
    relationship: "port",
  },
  transformers: {
    project: "Transformers",
    authors: "contributors of Hugging Face Transformers",
    relationship: "port",
  },
  graphics: {
    project: "Osprey (typed scene library)",
    authors: "NIMBLESITE PTY LTD, Australia",
    relationship: "original",
  },
  fs: {
    project: "Osprey (directory walking and paths)",
    authors: "NIMBLESITE PTY LTD, Australia",
    relationship: "original",
  },
  json: {
    project: "Osprey (JSON codec)",
    authors: "NIMBLESITE PTY LTD, Australia",
    relationship: "original",
  },
  uitest: {
    project: "Osprey (UI test library)",
    authors: "NIMBLESITE PTY LTD, Australia",
    relationship: "original",
  },
  vscode: {
    project: "Osprey (VS Code editor SDK)",
    authors: "NIMBLESITE PTY LTD, Australia",
    relationship: "original",
  },
};

const packageLink = (id) => `${REPO}/tree/main/src/osprey/packages/${id}`;

function buildPackages(audit) {
  return Object.entries(audit.packages).map(([id, entry]) => {
    const info = PACKAGES[id];
    if (!info) throw new Error(`credits.mjs: name the authors of src/osprey/packages/${id}`);
    const upstream = entry.upstreams?.[0];
    const reference = entry.references?.[0];
    const ospreyUrl = packageLink(id);
    return {
      id,
      name: `@osprey/${id.replaceAll("_", "-")}`,
      project: info.project,
      projectUrl: reference?.repository ?? upstream?.repository ?? null,
      authors: info.authors,
      license: entry.license,
      // Only a licence text we can point at gets a link: this column is the
      // Osprey package's licence, and must not read as the upstream's.
      licenseUrl: info.licenseUrl ?? upstream?.source ?? null,
      ospreyUrl,
      relationship: info.relationship,
    };
  });
}

// Apps and examples that derive from outside work but have no license-audit
// entry. Facts come from the repository's README/NOTICE records or the named
// upstream project; see each item's `basis` for where the attribution lives.
const APPS = [
  {
    name: "Classifier",
    project: "micrograd demo notebook",
    projectUrl: "https://github.com/karpathy/micrograd",
    authors: "Andrej Karpathy",
    license: "MIT",
    licenseUrl: "https://github.com/karpathy/micrograd",
    ospreyUrl: `${REPO}/tree/main/src/osprey/examples/python/ports/classifier`,
    relationship: "port",
    basis: "src/osprey/examples/python/ports/NOTICE.md",
  },
  {
    name: "Vector search",
    project: "FAISS flat-index tutorial",
    projectUrl: "https://github.com/facebookresearch/faiss",
    authors: "Meta Platforms, Inc. and affiliates",
    license: "MIT",
    licenseUrl: "https://github.com/facebookresearch/faiss",
    ospreyUrl: `${REPO}/tree/main/src/osprey/examples/python/ports/vector-search`,
    relationship: "port",
    basis: "src/osprey/examples/python/ports/NOTICE.md",
  },
  {
    name: "Text evaluation",
    project: "Google Research ROUGE application",
    projectUrl: "https://github.com/google-research/google-research/tree/master/rouge",
    authors: "The Google Research Authors",
    license: "Apache-2.0",
    licenseUrl: "https://github.com/google-research/google-research/tree/master/rouge",
    ospreyUrl: `${REPO}/tree/main/src/osprey/examples/python/ports/text-evaluation`,
    relationship: "port",
    basis: "src/osprey/examples/python/ports/NOTICE.md",
  },
  {
    name: "Painting",
    project: "GPUI painting example",
    projectUrl: "https://github.com/zed-industries/zed/tree/main/crates/gpui",
    authors: "contributors of GPUI (Zed Industries)",
    license: "Apache-2.0",
    licenseUrl: "https://github.com/zed-industries/zed/tree/main/crates/gpui",
    ospreyUrl: `${REPO}/tree/main/src/osprey/examples/gpui/painting`,
    relationship: "port",
    basis: "src/osprey/examples/gpui/painting/README.md",
  },
  {
    name: "GPUI host",
    project: "GPUI (gpui.rs)",
    projectUrl: "https://github.com/zed-industries/zed/tree/main/crates/gpui",
    authors: "contributors of GPUI (Zed Industries)",
    license: "Apache-2.0",
    licenseUrl: "https://github.com/zed-industries/zed/tree/main/crates/gpui",
    ospreyUrl: `${REPO}/tree/main/src/toolchain/hosts/gpui`,
    relationship: "binding",
    basis: "src/toolchain/hosts/gpui/README.md",
  },
  {
    name: "Osprey Agent",
    project: "pydantic-ai (Agent.run_sync)",
    projectUrl: "https://github.com/pydantic/pydantic-ai",
    authors: "Pydantic Services Inc. and the pydantic-ai contributors",
    license: "MIT",
    licenseUrl: "https://github.com/pydantic/pydantic-ai/blob/v2.46.0/LICENSE",
    ospreyUrl: `${REPO}/tree/main/src/osprey/examples/agent`,
    relationship: "port",
    basis: "src/osprey/examples/agent/README.md",
  },
  {
    name: "GPT-2",
    project: "GPT-2 (124M)",
    projectUrl: "https://huggingface.co/openai-community/gpt2",
    authors: "OpenAI",
    license: "MIT",
    licenseUrl: "https://huggingface.co/openai-community/gpt2",
    ospreyUrl: `${REPO}/tree/main/src/osprey/examples/llms/gpt2`,
    relationship: "port",
    basis: "src/osprey/examples/llms/gpt2/README.md",
  },
  {
    name: "Mathlib port and Proof Workbench",
    project: "Lean 4 and Mathlib",
    projectUrl: "https://github.com/leanprover/lean4",
    authors: "the Lean and Mathlib developers",
    license: "Apache-2.0",
    licenseUrl: "https://github.com/leanprover/lean4",
    ospreyUrl: `${REPO}/tree/main/src/osprey/examples/proof`,
    relationship: "port",
    basis: "src/toolchain/website/src/docs/mathlib-port.md",
  },
  {
    name: "Local coding model",
    project: "Qwen/Qwen3.8-27B",
    projectUrl: "https://huggingface.co/Qwen/Qwen3.8-27B",
    authors: "contributors of Qwen3.8-27B",
    license: "Apache-2.0",
    licenseUrl: "https://huggingface.co/Qwen/Qwen3.8-27B",
    ospreyUrl: `${REPO}/blob/main/docs/plans/0049-local-coding-model.md`,
    relationship: "port (planned)",
    basis: "docs/plans/0049-local-coding-model.md",
  },
];

export default function () {
  const audit = JSON.parse(readFileSync(AUDIT, "utf8"));
  const packages = buildPackages(audit);
  return {
    auditDate: audit.audit_date,
    ported: packages.filter((p) => p.relationship !== "original"),
    original: packages.filter((p) => p.relationship === "original"),
    apps: APPS,
  };
}
