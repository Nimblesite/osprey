// Osprey supplies source classification; all layout and interaction lives in lspkit.
export * from "@nimblesite/lsp-profiler-ui/out/src/flame-model";
import * as shared from "@nimblesite/lsp-profiler-ui/out/src/flame-model";
export const isOspreySource = (file: string): boolean =>
  file.endsWith(".osp") || file.endsWith(".ospml") || file.endsWith(".py");
export const buildFlameModel = (doc: shared.SpeedscopeFile): shared.FlameModel => shared.buildFlameModel(doc, isOspreySource);
