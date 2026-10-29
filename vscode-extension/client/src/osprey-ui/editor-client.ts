// Mechanical text-control behaviour. These functions run inside the existing
// webview closure, sharing its issued controls and pending-edit protocol.
export const EDITOR_SCRIPT = String.raw`
  let lastCaret = "";

  function editorMirror(element) {
    return element.parentElement.querySelector(".osp-editor-mirror");
  }

  function mirrorEdit(element) {
    const mirror = editorMirror(element);
    if (!mirror) throw new Error("The marked editor has no presentation surface");
    mirror.textContent = element.value + (element.value.endsWith("\n") ? "\u200b" : "");
    mirror.scrollTop = element.scrollTop;
    mirror.scrollLeft = element.scrollLeft;
  }

  function cursorBytes(element) {
    return new TextEncoder().encode(element.value.slice(0, element.selectionStart)).length;
  }

  function reportCaret() {
    const element = document.activeElement;
    if (!element || !element.matches("textarea[data-native-editor]")) return;
    if (pendingEdits.has(fieldSignature(element))) return;
    const control = controlOf(element);
    if (!control) throw new Error("The marked editor has no issued provenance");
    const offset = cursorBytes(element);
    const key = JSON.stringify([control.region, control.revision, element.dataset.placeOrdinal, offset]);
    if (offset === Number(element.dataset.cursorByte) || key === lastCaret) return;
    lastCaret = key;
    vscode.postMessage(Object.assign({}, control, { type: "event", ordinal: Number(element.dataset.placeOrdinal), value: offset }));
  }

  function refreshEditors() {
    document.querySelectorAll("textarea[data-native-editor]").forEach(function (element) {
      const pending = pendingEdits.get(fieldSignature(element));
      if (pending) mirrorEdit(element);
      element.addEventListener("scroll", function () {
        const mirror = editorMirror(element);
        mirror.scrollTop = element.scrollTop;
        mirror.scrollLeft = element.scrollLeft;
      });
    });
    queueMicrotask(reportCaret);
  }

  document.addEventListener("input", function (event) {
    const element = event.target;
    if (element.matches && element.matches("textarea[data-native-editor]")) mirrorEdit(element);
  });
  document.addEventListener("selectionchange", reportCaret);
`;
