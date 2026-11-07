# osprey: flavor=python
# osprey: namespace=qwen_server
# osprey: module=Prompts
"""Rendering a turn into the Qwen chat framing the fixture model was trained on.

The served format is Qwen3.5's: `<|im_start|>role`, the content, `<|im_end|>`,
and one newline, with the assistant turn opened and left for the model to
continue. This is the framing subset only: the unmodified
`qwen3.8-27b.chat_template.jinja` (tools, `reasoning_effort`,
`preserve_thinking`) arrives with plan 0049 Phase 5's jinja2 renderer, and until
then the server advertises no `template` in `/api/show` rather than shipping a
rendered approximation of one ([LLM-API-OLLAMA]).
"""

from qwen_server import Wire

__all__ = ["render", "render_single", "turn", "OPEN"]

OPEN = "<|im_start|>"
"""The frame every served message opens with."""
CLOSE = "<|im_end|>"
ASSISTANT = f"{OPEN}assistant\n"


def render(messages):
    """Every message as one turn, then the assistant prefix to continue."""
    return foldList(messages, "", lambda rendered, message: rendered + turn(message)) + ASSISTANT


def render_single(prompt, system):
    """`/api/generate`'s single user turn, with an optional system turn before it."""
    return system_turn(system) + turn(user_turn(prompt)) + ASSISTANT


def user_turn(prompt):
    """One `user` message carrying `prompt`."""
    return JsonObject([Wire.text_member("role", "user"), Wire.text_member("content", prompt)])


def system_turn(system):
    """The system turn, or nothing when the request named no system prompt."""
    return "" if system == "" else turn(JsonObject([Wire.text_member("role", "system"), Wire.text_member("content", system)]))


def turn(message):
    """One rendered message; a message without a role is a `user` turn. The content is
    trimmed: an answer that begins with newlines (as one does after `</think>`) would
    otherwise merge with the newline that ends the `assistant` prefix into one BPE
    token, so a history carrying that answer would no longer begin with the prompt's
    tokens and the cache checkpoint taken after the prompt could not be reused
    ([LLM-CACHE-CHECKPOINT])."""
    role = Wire.text(message, "role")
    content = trim(Wire.text(message, "content"))
    spelled = "user" if role == "" else role
    return f"{OPEN}{spelled}\n{content}\n{CLOSE}\n"
