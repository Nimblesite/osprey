# osprey: flavor=python
# osprey: namespace=pydantic_ai
# osprey: module=Direct
"""pydantic_ai.direct: one model request without an agent.

Port of pydantic_ai/direct.py (2.46.0). `model_request` calls the model's
request function. `model_request_stream` has no streaming transport in the
runtime, so it performs the same request and answers a `StreamedResponse`
whose `events` replay the complete response as `PartStartEvent`s (each text
part opening with its whole content) followed by `PartEndEvent`s; `get` is
`StreamedResponse.get()`.
"""
from pydantic_ai import Exceptions as E
from pydantic_ai import Messages as M
from pydantic_ai import Models

__all__ = ["StreamedResponse", "model_request", "model_request_stream", "get", "events_of"]


class StreamedResponse:
    """A streamed response: its events in order and the complete response."""
    events: list[M.ModelResponseStreamEvent]
    response: M.ModelResponse


def model_request(model, messages, model_settings, model_request_parameters) -> E.Outcome[M.ModelResponse]:
    """`model_request(model, messages, model_settings=, model_request_parameters=)`."""
    return model.request(messages, model_settings, model_request_parameters)


def events_of(response) -> list[M.ModelResponseStreamEvent]:
    """The events replaying a complete response: a start per part, then an end per part."""
    starts = M.map_indexed(response.parts, lambda i: M.PartStartEvent(index=i, part=unwrap_or(listGet(response.parts, i), M.TextPart(content=""))))
    ends = M.map_indexed(response.parts, lambda i: M.PartEndEvent(index=i, part=unwrap_or(listGet(response.parts, i), M.TextPart(content=""))))
    return listConcat(starts, ends)


def model_request_stream(model, messages, model_settings, model_request_parameters) -> E.Outcome[StreamedResponse]:
    """`model_request_stream(...)`: the response as events plus the whole response."""
    match model_request(model, messages, model_settings, model_request_parameters):
        case E.Ok(value=response):
            return E.Ok(value=StreamedResponse(events=events_of(response), response=response))
        case E.Failed(error=error):
            return E.Failed(error=error)


def get(stream) -> M.ModelResponse:
    """`StreamedResponse.get()`: the complete response."""
    return stream.response
