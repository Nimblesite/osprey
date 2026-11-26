// Implements [NATIVE-SHM-EVENT] caret words: a marked editor reports its caret as one UTF-8 byte
// offset, or its selection as "start end", on its place ordinal.

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
