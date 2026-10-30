// The webview's only script: generic focus preservation, pending edit rebasing,
// event delegation, splitter dragging and context-menu opening. It holds no
// application state and sends no capability request; every value is attributed
// to the region, revision and ordinal the Node renderer emitted.

import { EDITOR_SCRIPT } from "./editor-client";
import { BOARD_SCRIPT } from "./board-client";
import { ACTION_SCRIPT } from "./action-client";

export const WEBVIEW_SCRIPT = `
(function () {
  const vscode = acquireVsCodeApi();
  const fieldTimers = new Map();
  const pendingEdits = new Map();
  const submittedFields = new WeakSet();
  let savedFocus = null;
  let drag = null;
  let pendingLoss = null;
  let editSequence = 0;
  let hostDiagnostics = [];
  let runtimeBusy = false;
  let runtimeTracked = false;
  let pressedAction = null;
  let pressedPointerId = null;
  let cancelledAction = null;
  let pendingActionPatch = null;
  const pausedControls = new Map();

  function updateBusy() {
    const root = document.getElementById("osprey-root"), notice = document.getElementById("osprey-runtime-status");
    const busy = runtimeBusy || pendingEdits.size > 0;
    if (notice) notice.hidden = !busy;
    if (!root) return;
    root.setAttribute("aria-busy", String(busy));
    root.querySelectorAll("button, select").forEach(function (control) {
      if (control === pressedAction) return;
      if (runtimeBusy) {
        if (!pausedControls.has(control)) pausedControls.set(control, control.disabled);
        control.disabled = true;
      } else if (pausedControls.has(control)) control.disabled = pausedControls.get(control);
    });
    if (!runtimeBusy) pausedControls.clear();
  }

  function closest(target, selector) {
    return target && target.closest ? target.closest(selector) : null;
  }

  function controlOf(element) {
    const region = element.getAttribute("data-event-region");
    const revision = Number(element.getAttribute("data-event-revision"));
    const ordinal = Number(element.getAttribute("data-event-ordinal"));
    return region && Number.isInteger(revision) && Number.isInteger(ordinal)
      ? { region: region, revision: revision, ordinal: ordinal } : null;
  }

  function postEvent(element, override) {
    const control = controlOf(element);
    if (!control) return;
    let value;
    if (override !== undefined) {
      value = override;
    } else {
      const kind = element.getAttribute("data-value-kind");
      const raw = element.getAttribute("data-value");
      value = kind === "number" ? Number(raw) : kind === "bool" ? raw === "true" : kind === "string" ? raw : null;
    }
    if (postAfterFields(control, value)) return;
    if (runtimeTracked) { runtimeBusy = true; updateBusy(); }
    vscode.postMessage(Object.assign({ type: "event", value: value }, control));
  }

  function postAfterFields(control, value) {
    if (pendingEdits.size === 0 && !pendingActionPatch) return false;
    const pending = flushFields();
    pending.forEach(function (_edit, signature) { sendField(signature); });
    const id = ++editSequence;
    const signature = "action:" + id;
    pendingEdits.set(signature, { id: id, value: value, control: control, sent: false });
    if (runtimeTracked) runtimeBusy = true;
    updateBusy();
    sendField(signature);
    return true;
  }

  // Presentation matching only preserves the draft's display. Osprey resolves
  // the original issued binding; the browser never retargets an edit.
  function fieldSignature(element) {
    const label = element.closest ? element.closest("label") : null;
    const labelSpan = label && label.querySelector("span");
    return JSON.stringify([
      element.getAttribute("data-event-region"),
      element.getAttribute("data-focus"),
      element.tagName,
      element.getAttribute("type"),
      element.getAttribute("data-value-kind"),
      element.getAttribute("data-commit"),
      element.getAttribute("placeholder") || "",
      labelSpan ? labelSpan.textContent : "",
    ]);
  }

  function sendField(signature) {
    const edit = pendingEdits.get(signature);
    if (!edit || edit.sent) return;
    pendingEdits.set(signature, Object.assign({}, edit, { sent: true }));
    vscode.postMessage(Object.assign({ type: "event", edit: edit.id, value: edit.value }, edit.control));
  }

  function armFieldTimer(signature) {
    const previous = fieldTimers.get(signature);
    if (previous) clearTimeout(previous);
    fieldTimers.set(signature, setTimeout(function () {
      fieldTimers.delete(signature);
      sendField(signature);
    }, 160));
  }

  // Pending edits survive a re-render: take them before the DOM is replaced.
  // Keep drafts until the serialized host explicitly acknowledges handling.
  function flushFields() {
    fieldTimers.forEach(function (timer) { clearTimeout(timer); });
    fieldTimers.clear();
    const pending = new Map(pendingEdits);
    return pending;
  }

  function fieldsBySignature(signature) {
    return Array.prototype.filter.call(document.querySelectorAll("input, textarea, select"),
      function (element) { return element.getAttribute && fieldSignature(element) === signature; });
  }

  function rebaseField(signature, edit) {
    const matches = fieldsBySignature(signature);
    if (matches.length === 1) matches[0].value = edit.value;
    sendField(signature);
  }

  function rebasePendingEdits(pending) {
    pending.forEach(function (edit, signature) {
      rebaseField(signature, edit);
    });
  }

  function renderDiagnostics(messages) {
    hostDiagnostics = Array.isArray(messages) ? messages : [];
    const combined = pendingLoss === null ? hostDiagnostics : hostDiagnostics.concat([pendingLoss]);
    const diagnostics = document.getElementById("osprey-diagnostics");
    if (!diagnostics) throw new Error("The workspace diagnostics element is absent");
    diagnostics.textContent = Array.from(new Set(combined)).join("\\n");
    diagnostics.hidden = combined.length === 0;
  }

  function acknowledgeEdit(message) {
    if (acknowledgeBoardEdit(message)) return;
    const entry = Array.from(pendingEdits.entries()).find(function (entry) { return entry[1].id === message.edit; });
    if (!entry) return;
    const signature = entry[0];
    clearTimeout(fieldTimers.get(signature));
    fieldTimers.delete(signature);
    pendingEdits.delete(signature);
    updateBusy();
    if (message.accepted === true) return;
    pendingLoss = message.error || "The text edit was refused; it was not applied.";
    fieldsBySignature(signature).forEach(function (element) { element.value = element.getAttribute("data-value") || ""; });
    renderDiagnostics(hostDiagnostics);
  }

  function closeMenus() {
    document.querySelectorAll(".osp-menu").forEach(function (menu) {
      menu.hidden = true;
      menu.style.left = "";
      menu.style.top = "";
    });
  }

  function openMenu(host, x, y) {
    const menu = host.querySelector(".osp-menu");
    if (!menu) return;
    closeMenus();
    menu.hidden = false;
    menu.style.left = Math.max(8, Math.min(x, window.innerWidth - 260)) + "px";
    menu.style.top = Math.max(8, Math.min(y, window.innerHeight - 240)) + "px";
  }

  function saveFocus() {
    savedFocus = null;
    if (!document.hasFocus()) return;
    const active = document.activeElement;
    if (!active || !active.getAttribute) return;
    const id = active.getAttribute("data-focus");
    if (!id) return;
    savedFocus = {
      id: id,
      start: typeof active.selectionStart === "number" ? active.selectionStart : null,
      end: typeof active.selectionEnd === "number" ? active.selectionEnd : null,
      signature: active.matches("input, textarea") ? fieldSignature(active) : null,
    };
  }

  let dialogReturnFocus = null;
  function focusDialog() {
    const dialog = document.querySelector("[data-native-dialog]");
    if (!document.hasFocus()) { if (!dialog) dialogReturnFocus = null; return; }
    if (dialog) {
      if (dialogReturnFocus === null) dialogReturnFocus = document.activeElement?.getAttribute("data-focus") || "";
      if (!dialog.contains(document.activeElement)) (dialog.querySelector("button, input, textarea, select, a[href]") || dialog).focus();
    } else if (dialogReturnFocus !== null) {
      const focus = Array.from(document.querySelectorAll("[data-focus]")).find(function (node) { return node.getAttribute("data-focus") === dialogReturnFocus; });
      if (focus) focus.focus();
      dialogReturnFocus = null;
    }
  }

  function restoreFocus() {
    const saved = savedFocus;
    savedFocus = null;
    if (!saved) return;
    const element = Array.prototype.find.call(document.querySelectorAll("[data-focus]"), function (candidate) {
      return candidate.getAttribute("data-focus") === saved.id;
    });
    if (!element || !element.focus) return;
    if (saved.signature !== null && fieldSignature(element) !== saved.signature) return;
    element.focus();
    if (saved.start !== null && element.setSelectionRange) {
      try { element.setSelectionRange(saved.start, saved.end === null ? saved.start : saved.end); } catch (_) { /* not a text control */ }
    }
  }

  function elementPath(element) {
    const path = [];
    let node = element;
    while (node && node.parentElement && node !== document.body && node !== document.documentElement) {
      path.unshift(Array.prototype.indexOf.call(node.parentElement.children, node));
      node = node.parentElement;
    }
    return path.join(".");
  }

  function elementAtPath(path) {
    if (path === "") return document.scrollingElement;
    let node = document.body;
    const steps = path.split(".");
    for (let index = 0; index < steps.length; index += 1) {
      if (!node) return null;
      node = node.children[Number(steps[index])];
    }
    return node || null;
  }

  function scrollable() {
    const found = [document.scrollingElement].concat(Array.prototype.slice.call(document.querySelectorAll("*")));
    return found.filter(function (element) { return element && (element.scrollTop || element.scrollLeft); });
  }

  function captureScroll() {
    return scrollable().map(function (element) {
      return { path: elementPath(element), top: element.scrollTop, left: element.scrollLeft };
    });
  }

  function restoreScroll(saved) {
    saved.forEach(function (entry) {
      const element = elementAtPath(entry.path);
      if (!element) return;
      element.scrollTop = entry.top;
      element.scrollLeft = entry.left;
    });
  }

${ACTION_SCRIPT}

  document.addEventListener("click", function (event) {
    if (event instanceof PointerEvent && event.detail !== 0 && pressedPointerId !== null && event.pointerId !== pressedPointerId) return;
    const external = closest(event.target, "a[data-external]");
    if (external) {
      event.preventDefault();
      vscode.postMessage({ type: "external", url: external.getAttribute("href") });
      return;
    }
    closeMenus();
    const element = closest(event.target, "[data-event-region]");
    if (element && element === cancelledAction && event.detail !== 0) { releaseAction(); return; }
    if (!element || element.hasAttribute("data-splitter") || element.hasAttribute("data-native-dialog")) return;
    if (element.matches("input, textarea, select, canvas")) return;
    postEvent(element);
    releaseAction();
  });

  document.addEventListener("input", function (event) {
    const element = event.target;
    if (!element.matches || !element.matches("input, textarea")) return;
    submittedFields.delete(element);
    if (element.getAttribute("data-commit") !== "edit") return;
    pendingLoss = null;
    const signature = fieldSignature(element);
    const control = controlOf(element);
    if (!control) throw new Error("The edited text control has no issued provenance");
    pendingEdits.set(signature, {
      id: ++editSequence,
      value: element.value,
      control: control,
      sent: false,
    });
    updateBusy();
    armFieldTimer(signature);
  });

  document.addEventListener("change", function (event) {
    const element = event.target;
    if (!element.matches || !element.matches("select, [data-commit='submit']")) return;
    if (submittedFields.delete(element)) return;
    postEvent(element, element.value);
  });

  document.addEventListener("keydown", function (event) {
    if (event.key === "Escape") {
      const dialog = document.querySelector("[data-native-dialog]");
      if (dialog) { event.preventDefault(); postEvent(dialog, null); }
      closeMenus(); return;
    }
    const modal = document.querySelector("[data-native-dialog]");
    if (modal && event.key === "Tab") {
      const fields = Array.from(modal.querySelectorAll("button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), a[href]"));
      const index = fields.indexOf(document.activeElement);
      const next = event.shiftKey ? (index <= 0 ? fields.length - 1 : index - 1) : (index + 1) % fields.length;
      event.preventDefault(); (fields[next] || modal).focus(); return;
    }
    const element = event.target;
    if (!element || !element.getAttribute) return;
    if (element.hasAttribute("data-splitter") && /^Arrow(Left|Right)$/.test(event.key)) {
      const current = Number(element.getAttribute("data-value"));
      const min = Number(element.getAttribute("data-min"));
      const max = Number(element.getAttribute("data-max"));
      const side = element.getAttribute("data-side");
      const step = event.key === "ArrowRight" ? 20 : -20;
      const next = Math.max(min, Math.min(max, current + (side === "leading" ? step : -step)));
      element.setAttribute("data-value", String(next));
      const aside = closest(element, ".osp-aside");
      if (aside) aside.style.setProperty("--osp-aside-width", next + "px");
      postEvent(element, next);
      event.preventDefault();
      return;
    }
    if (event.key === "Enter" && element.matches && element.matches("[data-commit='submit']") && element.tagName !== "TEXTAREA") {
      submittedFields.add(element);
      postEvent(element, element.value);
      event.preventDefault();
    }
  });

  document.addEventListener("contextmenu", function (event) {
    const host = closest(event.target, "[data-context-host]");
    if (!host) return;
    event.preventDefault();
    openMenu(host, event.clientX, event.clientY);
  });

  document.addEventListener("keydown", function (event) {
    if (event.key !== "ContextMenu" && !(event.shiftKey && event.key === "F10")) return;
    const host = closest(document.activeElement, "[data-context-host]");
    if (!host) return;
    event.preventDefault();
    const box = host.getBoundingClientRect();
    openMenu(host, box.left + 24, box.top + 24);
  });

  document.addEventListener("pointerdown", function (event) {
    const splitter = closest(event.target, "[data-splitter]");
    if (!splitter) return;
    event.preventDefault();
    drag = {
      splitter: splitter,
      startX: event.clientX,
      startWidth: Number(splitter.getAttribute("data-value")),
      width: Number(splitter.getAttribute("data-value")),
      min: Number(splitter.getAttribute("data-min")),
      max: Number(splitter.getAttribute("data-max")),
      side: splitter.getAttribute("data-side"),
    };
    splitter.classList.add("osp-dragging");
    if (splitter.setPointerCapture) splitter.setPointerCapture(event.pointerId);
  });

  document.addEventListener("pointermove", function (event) {
    if (!drag) return;
    const delta = event.clientX - drag.startX;
    drag.width = Math.max(drag.min, Math.min(drag.max, drag.startWidth + (drag.side === "leading" ? delta : -delta)));
    const aside = closest(drag.splitter, ".osp-aside");
    if (aside) aside.style.setProperty("--osp-aside-width", drag.width + "px");
  });

  document.addEventListener("pointerup", function () {
    if (!drag) return;
    const finished = drag;
    drag = null;
    finished.splitter.classList.remove("osp-dragging");
    if (Number.isFinite(finished.width)) postEvent(finished.splitter, finished.width);
  });

  function renderMessage(message) {
    if (boardDrawing) { deferredBoardRender = message; return; }
    const pending = flushFields();
    saveFocus();
    const scroll = captureScroll();
    const root = document.getElementById("osprey-root");
    if (root && (!pressedAction || !patchHeldAction(root, message.html))) {
      if (pressedAction) releaseAction();
      root.innerHTML = message.html;
    }
    restoreFocus();
    focusDialog();
    rebasePendingEdits(pending);
    refreshBoards();
    restoreScroll(scroll);
    refreshEditors();
    renderDiagnostics(message.diagnostics);
    updateBusy();
  }

  window.addEventListener("message", function (event) {
    const message = event.data;
    if (!message || typeof message !== "object") return;
    if (message.type === "busy") { runtimeTracked = true; runtimeBusy = message.active === true; updateBusy(); return; }
    if (message.type === "render") { renderMessage(message); return; }
    if (message.type === "edit-result") { acknowledgeEdit(message); return; }
    if (message.type === "fatal") {
      cancelAction();
      flushFields();
      pendingEdits.clear();
      runtimeBusy = false;
      updateBusy();
      pendingLoss = message.message;
      renderDiagnostics(hostDiagnostics);
    }
  });

${EDITOR_SCRIPT}
${BOARD_SCRIPT}
  vscode.postMessage({ type: "ready" });
})();
`;
