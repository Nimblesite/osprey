import React from "react";

const EVENT_NAMES = new Set([
  "blur",
  "change",
  "click",
  "doubleClick",
  "focus",
  "input",
  "keyDown",
  "keyUp",
  "pointerDown",
  "pointerUp",
  "submit",
]);

const VOID_TAGS = new Set([
  "area",
  "base",
  "br",
  "col",
  "embed",
  "hr",
  "img",
  "input",
  "link",
  "meta",
  "param",
  "source",
  "track",
  "wbr",
]);

const BOOLEAN_PROPS = new Set([
  "autoFocus",
  "checked",
  "disabled",
  "hidden",
  "multiple",
  "readOnly",
  "required",
  "selected",
]);

const PROP_ALIASES = {
  class: "className",
  for: "htmlFor",
  tabindex: "tabIndex",
  readonly: "readOnly",
  maxlength: "maxLength",
  autocomplete: "autoComplete",
};

function eventPropName(name) {
  // React treats a value-bearing input with only `onInput` as read-only. The
  // protocol retains kind=input while React receives the canonical onChange.
  if (name === "input") return "onChange";
  if (name === "doubleClick") return "onDoubleClick";
  return `on${name.charAt(0).toUpperCase()}${name.slice(1)}`;
}

function domValue(name, value) {
  if (name === "style") return parseStyle(value);
  if (BOOLEAN_PROPS.has(name) && (value === "true" || value === "false")) {
    return value === "true";
  }
  return value;
}

function formDataJson(form) {
  const values = {};
  for (const [name, value] of new FormData(form).entries()) {
    const scalar = typeof File !== "undefined" && value instanceof File ? value.name : value;
    if (Object.hasOwn(values, name)) {
      values[name] = Array.isArray(values[name])
        ? [...values[name], scalar]
        : [values[name], scalar];
    } else {
      values[name] = scalar;
    }
  }
  return JSON.stringify(values);
}

function eventValue(kind, target, staticProps, browserEvent) {
  if (kind === "input" || kind === "change") {
    if (target?.type === "checkbox" || target?.type === "radio") {
      return target.checked ? "true" : "false";
    }
    if (target && "value" in target) return String(target.value);
    return undefined;
  }
  if ((kind === "keyDown" || kind === "keyUp") && browserEvent.key) {
    return browserEvent.key;
  }
  return staticProps.value === undefined ? undefined : String(staticProps.value);
}

function preventAnchorClick(kind, target, browserEvent) {
  const isAnchor = typeof HTMLAnchorElement !== "undefined" && target instanceof HTMLAnchorElement;
  if (kind === "click" && isAnchor) browserEvent.preventDefault();
}

function makeEvent(kind, id, staticProps, browserEvent) {
  const target = browserEvent.currentTarget;
  const message = { kind };
  if (id) message.id = String(id);
  if (kind === "submit") {
    browserEvent.preventDefault();
    message.data = formDataJson(target);
    return message;
  }
  preventAnchorClick(kind, target, browserEvent);
  const name = target?.name || staticProps.name;
  const value = eventValue(kind, target, staticProps, browserEvent);
  if (name) message.name = String(name);
  if (value !== undefined) message.value = value;
  return message;
}

function dispatchBrowserEvent(kind, source, browserEvent, send) {
  if (
    kind === "click" &&
    source.id === "modal-backdrop" &&
    browserEvent.target !== browserEvent.currentTarget
  ) {
    return;
  }
  browserEvent.stopPropagation?.();
  send(makeEvent(kind, source.id, source, browserEvent));
}

function parseStyle(value) {
  if (!value || typeof value !== "string") return value;
  return Object.fromEntries(
    value
      .split(";")
      .map((declaration) => declaration.split(":"))
      .filter(([name, part]) => name?.trim() && part !== undefined)
      .map(([name, ...parts]) => [
        name.trim().replace(/-([a-z])/g, (_match, letter) => letter.toUpperCase()),
        parts.join(":").trim(),
      ]),
  );
}

function safeDomProps(source) {
  const props = {};
  for (const [originalName, value] of Object.entries(source)) {
    const reserved = originalName === "event" || originalName === "events";
    if (reserved || originalName === "dangerouslySetInnerHTML") continue;
    const name = PROP_ALIASES[originalName.toLowerCase()] ?? originalName;
    if (/^on[A-Z]/.test(name)) continue;
    props[name] = domValue(name, value);
  }
  return props;
}

function addPrimaryEvent(props, source, send) {
  const event = source.event;
  if (typeof event === "string" && EVENT_NAMES.has(event)) {
    props[eventPropName(event)] = (browserEvent) =>
      dispatchBrowserEvent(event, source, browserEvent, send);
  }
}

function extendedHandler(name, descriptor, source, send) {
  return (browserEvent) => {
    const message = makeEvent(name, source.id, source, browserEvent);
    if (typeof descriptor === "string") message.id = descriptor;
    else if (descriptor && typeof descriptor === "object") Object.assign(message, descriptor);
    send(message);
  };
}

function addExtendedEvents(props, source, send) {
  if (source.events && typeof source.events === "object") {
    for (const [name, descriptor] of Object.entries(source.events)) {
      if (!EVENT_NAMES.has(name)) continue;
      props[eventPropName(name)] = extendedHandler(name, descriptor, source, send);
    }
  }
}

function domProps(source, send) {
  const props = safeDomProps(source);
  addPrimaryEvent(props, source, send);
  addExtendedEvents(props, source, send);
  return props;
}

function renderChildren(node, send, keyPath) {
  const children = [];
  if (node.text !== undefined && node.text !== null && node.text !== "") {
    children.push(String(node.text));
  }
  if (Array.isArray(node.children)) {
    node.children.forEach((child, index) => {
      children.push(renderNode(child, send, `${keyPath}.${index}`));
    });
  }
  return children;
}

function renderNode(node, send, keyPath) {
  if (node === null || node === undefined || node === false) return null;
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) {
    return node.map((child, index) => renderNode(child, send, `${keyPath}.${index}`));
  }
  if (typeof node !== "object") return null;

  const tag = typeof node.tag === "string" && node.tag ? node.tag : "div";
  const sourceProps = node.props && typeof node.props === "object" ? node.props : {};
  const props = domProps(sourceProps, send);
  props.key = node.key ?? sourceProps.key ?? keyPath;

  if (VOID_TAGS.has(tag.toLowerCase())) return React.createElement(tag, props);
  return React.createElement(tag, props, ...renderChildren(node, send, keyPath));
}

export function nodeToReact(node, send) {
  return renderNode(node, send, "root");
}

export function normalizeEnvelope(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Osprey render message must be a JSON object");
  }
  return {
    model:
      typeof value.model === "string"
        ? value.model
        : JSON.stringify(value.model ?? {}),
    view: value.view ?? null,
    commands: Array.isArray(value.commands) ? value.commands : [],
  };
}

export function commandKind(command) {
  return command?.kind ?? command?.type ?? "";
}
