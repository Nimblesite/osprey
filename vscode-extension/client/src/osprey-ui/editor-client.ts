// Mechanical text-control behaviour. These functions run inside the existing
// webview closure, sharing its issued controls and pending-edit protocol.
export const EDITOR_SCRIPT = String.raw`
  let lastCaret = "";
  let editorPositions = new Map();

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

  function authoredCaret(element, previous) {
    const current = { value: element.value, cursor: Number(element.dataset.cursorByte) };
    editorPositions.set(fieldSignature(element), current);
    if (pendingEdits.has(fieldSignature(element)) || document.activeElement === element) return;
    const bytes = new TextEncoder().encode(current.value);
    if (current.cursor > bytes.length) throw new Error("The authored editor caret exceeds its source");
    const prefix = new TextDecoder("utf-8", { fatal: true }).decode(bytes.slice(0, current.cursor));
    element.setSelectionRange(prefix.length, prefix.length);
    if (!previous || previous.value !== current.value || previous.cursor !== current.cursor) revealEditorCaret(element, prefix);
  }

  function revealEditorCaret(element, prefix) {
    const box = editorCaretBounds(element, prefix), editor = element.getBoundingClientRect();
    if (box.top < editor.top) element.scrollTop += box.top - editor.top;
    if (box.bottom > editor.bottom) element.scrollTop += box.bottom - editor.bottom + 12;
    editorMirror(element).scrollTop = element.scrollTop;
  }

  function editorCaretBounds(element, prefix) {
    const mirror = editorMirror(element), saved = mirror.style.cssText, style = getComputedStyle(element);
    const lines = prefix.split("\n"), line = mirror.children[lines.length - 1];
    if (!line || !line.firstChild) throw new Error("The authored editor caret has no source line");
    mirror.style.setProperty("display", "block", "important");
    mirror.style.setProperty("visibility", "hidden", "important");
    ["font", "padding", "letter-spacing", "white-space", "overflow-wrap"].forEach(function (key) { mirror.style.setProperty(key, style.getPropertyValue(key), "important"); });
    try {
      const range = document.createRange();
      range.setStart(line.firstChild, lines[lines.length - 1].length);
      range.collapse(true);
      return range.getBoundingClientRect();
    } finally { mirror.style.cssText = saved; }
  }

  function refreshEditors() {
    const previous = editorPositions;
    editorPositions = new Map();
    document.querySelectorAll("textarea[data-native-editor]").forEach(function (element) {
      const pending = pendingEdits.get(fieldSignature(element));
      if (pending) mirrorEdit(element);
      authoredCaret(element, previous.get(fieldSignature(element)));
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
