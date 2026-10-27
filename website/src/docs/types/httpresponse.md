---
layout: page
mlTwins: manual
title: "HttpResponse (Type)"
description: "The record returned by an HTTP server handler: status, raw headers and a complete text body."
---

A native HTTP server handler returns `HttpResponse`. The runtime sends its status, raw headers and complete text body, computes `Content-Length`, and closes the connection.

| Field | Current behavior |
| --- | --- |
| `status` | HTTP status code. |
| `headers` | Raw header lines, each ending in `\r\n`. Set `Content-Type` here. |
| `partialBody` | The complete body sent to the client. |
| `contentType`, `streamFd`, `isComplete` | Required by the callback ABI; these fields do not affect transport. |

Streaming is not implemented. Use `streamFd: -1` and `isComplete: true`, as in the example. See the [HTTP runtime contract](/spec/0014-http/#server-http-server).

## Example

```osprey
let response = HttpResponse {
    status: 200,
    headers: "Content-Type: application/json\r\n",
    contentType: "application/json",
    streamFd: -1,
    isComplete: true,
    partialBody: "{\"message\": \"Hello\"}"
}
print("${response.status}: ${response.partialBody}")
```

```osprey-ml
response = HttpResponse
    status = 200
    headers = "Content-Type: application/json\r\n"
    contentType = "application/json"
    streamFd = -1
    isComplete = true
    partialBody = "{\"message\": \"Hello\"}"
print "${response.status}: ${response.partialBody}"
```

Both programs print `200: {"message": "Hello"}`. An HTTP request handler returns this record; constructing it alone does not send a response.
