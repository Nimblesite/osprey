// Preserve the actual pressed control and its issued binding while the rest of
// a frame updates. Presentation matching never supplies a replacement action.
export const ACTION_SCRIPT = String.raw`
  let actionSequence = 0;
  function copyActionAttributes(current, next) {
    Array.from(current.attributes).forEach(function (attribute) {
      if (!next.hasAttribute(attribute.name)) current.removeAttribute(attribute.name);
    });
    Array.from(next.attributes).forEach(function (attribute) { current.setAttribute(attribute.name, attribute.value); });
  }

  function heldActionPath(root) {
    const path = [];
    let node = pressedAction;
    while (node && node !== root) {
      if (!node.parentNode) return null;
      path.unshift(Array.prototype.indexOf.call(node.parentNode.childNodes, node));
      node = node.parentNode;
    }
    return node === root ? path : null;
  }

  function compatibleActionPath(root, next, path) {
    let current = root, candidate = next;
    for (const index of path) {
      if (current.childNodes.length !== candidate.childNodes.length) return false;
      current = current.childNodes[index];
      candidate = candidate.childNodes[index];
      if (current.nodeType !== candidate.nodeType || current.nodeName !== candidate.nodeName) return false;
    }
    return candidate.nodeType === 1 && !candidate.disabled && candidate.getAttribute("data-focus") === pressedAction.getAttribute("data-focus");
  }

  function patchActionBranch(current, next) {
    if (current === pressedAction) { pendingActionPatch = next; return; }
    if (current.nodeType === 1 && next.nodeType === 1) copyActionAttributes(current, next);
    const children = Array.from(current.childNodes), replacements = Array.from(next.childNodes);
    children.forEach(function (child, index) {
      if (child.contains(pressedAction)) patchActionBranch(child, replacements[index]);
      else child.replaceWith(replacements[index]);
    });
  }

  function patchHeldAction(root, html) {
    const template = document.createElement("template"), path = heldActionPath(root);
    template.innerHTML = html;
    if (!path || !compatibleActionPath(root, template.content, path)) return false;
    patchActionBranch(root, template.content);
    return true;
  }

  function releaseAction() {
    actionSequence += 1;
    const button = pressedAction || cancelledAction, next = pendingActionPatch;
    pressedAction = null;
    pressedPointerId = null;
    cancelledAction = null;
    pendingActionPatch = null;
    if (button && next && button.isConnected) {
      copyActionAttributes(button, next);
      button.replaceChildren.apply(button, Array.from(next.childNodes));
    }
    updateBusy();
  }

  function cancelAction() {
    const button = pressedAction || cancelledAction;
    releaseAction();
    cancelledAction = button;
  }

  document.addEventListener("pointerdown", function (event) {
    const button = closest(event.target, "button[data-event-region]");
    if (!event.isPrimary || event.button !== 0 || !button || button.disabled) return;
    if (pressedPointerId !== null && event.pointerId !== pressedPointerId) return;
    releaseAction();
    pressedAction = button;
    pressedPointerId = event.pointerId;
    sendPendingFields();
  });
  document.addEventListener("pointerup", function (event) {
    if (event.pointerId !== pressedPointerId) return;
    if (pressedAction && !pressedAction.contains(document.elementFromPoint(event.clientX, event.clientY))) cancelledAction = pressedAction;
    const sequence = actionSequence;
    setTimeout(function () { if (sequence === actionSequence) releaseAction(); }, 0);
  });
  document.addEventListener("pointercancel", function (event) { if (event.pointerId === pressedPointerId) cancelAction(); });
  document.addEventListener("pointermove", function (event) { if (event.pointerId === pressedPointerId && event.buttons === 0) releaseAction(); });
  window.addEventListener("blur", cancelAction);
`;
