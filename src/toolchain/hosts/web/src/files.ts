// The `choose-file` capability in the browser ([UI-EFFECTS]): the person picks a file with the
// page's own file chooser. A browser shows no paths, so the completion is the file's name, or ""
// when the chooser is cancelled, and the file's bytes land base64-encoded in the slice it named.

import { bare, limitOf, required, type Ask, type Completion } from "../../../vscode-extension/client/src/osprey-ui/asks";

const MOST_BYTES = 1 << 30;
const CHUNK = 0x8000;

function picked(types: readonly string[]): Promise<File | undefined> {
  const input = document.createElement("input");
  input.type = "file";
  input.accept = types.map((type) => `.${type}`).join(",");
  return new Promise((resolve) => {
    input.addEventListener("change", () => resolve(input.files?.[0]));
    input.addEventListener("cancel", () => resolve(undefined));
    input.click();
  });
}

function base64(bytes: Uint8Array): string {
  let binary = "";
  for (let at = 0; at < bytes.length; at += CHUNK) binary += String.fromCharCode(...bytes.subarray(at, at + CHUNK));
  return btoa(binary);
}

/** Let the person choose one file of `types`; its bytes, at most the request's limit, fill its slice. */
export async function chosen(ask: Ask): Promise<Completion> {
  const limit = limitOf(ask, 2, MOST_BYTES);
  const slice = required(ask, 3, "slice");
  const types = ask.texts(1);
  if (types.length === 0 || types.length > 64 || !types.every((type) => /^[a-z0-9][a-z0-9_-]*$/i.test(type))) throw new Error("choose-file has no valid file extension filter");
  const file = await picked(types);
  if (file === undefined) return bare("");
  if (file.size > limit) throw new Error(`${file.name} is ${file.size} bytes, over the ${limit} byte limit`);
  return { detail: file.name, slices: [{ key: slice, text: base64(new Uint8Array(await file.arrayBuffer())) }] };
}
