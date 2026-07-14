import test from "node:test";
import assert from "node:assert/strict";
import { nodeToReact, normalizeEnvelope, commandKind } from "../src/protocol.js";

class FormDataMock {
  constructor(form) {
    this.form = form;
  }

  entries() {
    return this.form.values[Symbol.iterator]();
  }
}

function protocolInput(messages) {
  return nodeToReact(
    {
      tag: "input",
      text: "",
      props: {
        id: "account-search", name: "search", value: "Amelia",
        required: "true", disabled: "false", event: "input",
      },
    },
    (message) => messages.push(message),
  );
}

function mapsProtocolInput() {
  const messages = [];
  const element = protocolInput(messages);
  assert.equal(element.type, "input");
  assert.equal(element.props.children, undefined);
  assert.equal(typeof element.props.onChange, "function");
  assert.equal(element.props.onInput, undefined);
  assert.equal(element.props.required, true);
  assert.equal(element.props.disabled, false);
  element.props.onChange({ currentTarget: { name: "search", value: "Priya", type: "text" } });
  assert.deepEqual(messages, [
    { kind: "input", id: "account-search", name: "search", value: "Priya" },
  ]);
}

function transferSubmitEvent(preventDefault) {
  return {
    currentTarget: { values: [["from", "1"], ["to", "2"], ["amount", "62.40"]] },
    preventDefault,
    stopPropagation() {},
  };
}

function captureTransferSubmission() {
  const messages = [];
  const element = nodeToReact(
    { tag: "form", props: { event: "submit", id: "submit-transfer" } },
    (message) => messages.push(message),
  );
  let prevented = false;
  element.props.onSubmit(transferSubmitEvent(() => {
    prevented = true;
  }));
  return { messages, prevented };
}

function serializesFormSubmission() {
  const NativeFormData = globalThis.FormData;
  globalThis.FormData = FormDataMock;
  try {
    const { messages, prevented } = captureTransferSubmission();
    assert.equal(prevented, true);
    assert.deepEqual(messages, [{
      kind: "submit",
      id: "submit-transfer",
      data: '{"from":"1","to":"2","amount":"62.40"}',
    }]);
  } finally {
    globalThis.FormData = NativeFormData;
  }
}

test("normalizes the opaque model and command list", () => {
  assert.deepEqual(
    normalizeEnvelope({ model: '{"route":"overview"}', view: { tag: "main" } }),
    {
      model: '{"route":"overview"}',
      view: { tag: "main" },
      commands: [],
    },
  );
  assert.equal(commandKind({ kind: "http", type: "focus" }), "http");
  assert.equal(commandKind({ type: "focus" }), "focus");
});

test("maps protocol input to one React onChange and preserves kind=input", mapsProtocolInput);

test("maps declarative clicks without leaking the event prop to the DOM", () => {
  const messages = [];
  const element = nodeToReact(
    { tag: "button", props: { event: "click", id: "nav-accounts", value: "accounts" }, text: "Accounts" },
    (message) => messages.push(message),
  );
  assert.equal(element.props.event, undefined);
  element.props.onClick({ currentTarget: { name: "", value: "accounts" } });
  assert.deepEqual(messages, [{ kind: "click", id: "nav-accounts", value: "accounts" }]);
});

test("serializes form submission data into the flat data field", serializesFormSubmission);

test("renders nested protocol nodes as React elements", () => {
  const element = nodeToReact(
    {
      tag: "section",
      props: { class: "card", style: "color: red; background-color: white" },
      children: [{ tag: "h2", text: "Accounts" }, { tag: "p", text: "Three open accounts" }],
    },
    () => {},
  );
  assert.equal(element.props.className, "card");
  assert.deepEqual(element.props.style, { color: "red", backgroundColor: "white" });
  assert.equal(element.props.children[0].type, "h2");
  assert.equal(element.props.children[0].props.children, "Accounts");
});
