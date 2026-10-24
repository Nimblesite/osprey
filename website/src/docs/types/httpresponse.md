---
layout: page
mlTwins: manual
title: "HttpResponse (Type)"
description: "A built-in type representing an HTTP response with status code, headers, content type, body, and streaming capabilities. Used by HTTP server handlers to return structured responses to clients."
---

**Description:** A built-in type representing an HTTP response with status code, headers, content type, body, and streaming capabilities. Used by HTTP server handlers to return structured responses to clients.

## Example

```osprey
let response = HttpResponse {
    status: 200,
    headers: "Content-Type: application/json",
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
    headers = "Content-Type: application/json"
    contentType = "application/json"
    streamFd = -1
    isComplete = true
    partialBody = "{\"message\": \"Hello\"}"
print "${response.status}: ${response.partialBody}"
```

Both programs print `200: {"message": "Hello"}`. An HTTP request handler returns this record; constructing it alone does not send a response.
