// The webview's form fields as the DOM holds them: the signature a draft is keyed by, a field's focus
// epoch, and a marked editor's caret words ([NATIVE-SHM-EVENT]): one UTF-8 byte offset, or a
// selection as "start end", on its place ordinal.

type Field = HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement;

function bytesTo(element: HTMLTextAreaElement, at: number): number {
  return new TextEncoder().encode(element.value.slice(0, at)).length;
}

/** The caret as "start", or a selection as "start end", in UTF-8 bytes. */
export function caretWords(element: HTMLTextAreaElement): string {
  const start = bytesTo(element, element.selectionStart), end = bytesTo(element, element.selectionEnd);
  return start === end ? String(start) : `${start} ${end}`;
}

/** Whether `element` is a marked editor's textarea. */
export function isEditor(element: EventTarget | null): element is HTMLTextAreaElement {
  return element instanceof HTMLTextAreaElement && element.matches("textarea[data-native-editor]");
}

/** The identity a draft is kept under: where the field is and what kind of field it is. */
export function fieldSignature(element: Element): string {
  const label = element.closest("label");
  const labelSpan = label?.querySelector("span") ?? null;
  return JSON.stringify([
    element.getAttribute("data-focus"),
    element.tagName,
    element.getAttribute("type"),
    element.getAttribute("data-value-kind"),
    element.getAttribute("data-commit"),
    element.getAttribute("placeholder") || "",
    labelSpan !== null ? labelSpan.textContent : "",
  ]);
}

/** Every field drawn under `signature`. */
export function fieldsBySignature(signature: string): Field[] {
  return Array.from(document.querySelectorAll<Field>("input, textarea, select"))
    .filter((element) => fieldSignature(element) === signature);
}

/** The focus epoch the frame gave `field`; a higher one re-authors it. */
export function epochOf(field: Element): number {
  return Number(field.getAttribute("data-focus-epoch") || 0);
}
