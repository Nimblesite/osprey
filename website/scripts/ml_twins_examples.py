"""Explicit ML renderings for reference examples requiring layout or patterns."""

JSON_DECODE = '''encode value = match jsonEncode value
    Success value => value
    Error message => message
match jsonDecode "[1]"
    Success value => print (encode value)
    Error message => print message'''

JSON_ENCODE = '''match jsonEncode (JsonArray [JsonInt 1])
    Success value => print value
    Error message => print message'''

JSON_DOCUMENT = '''inspect document =
    summary = "name=${jsonGet (document, "name")}; items=${jsonLength (document, "items")}"
    released = jsonFree document
    "${summary}; released=${released}"
match jsonParse "{\\"name\\":\\"osprey\\",\\"items\\":[1,2]}"
    Success value => print (inspect value)
    Error message => print message'''

HTTP_PREFIX = '''reply (_method, _path, _headers, _body) = HttpResponse(
    status = 200, headers = "X-Demo: hi\\r\\n", contentType = "text/plain",
    streamFd = -1, isComplete = true, partialBody = "hello")
runRequest client =
    result = '''

HTTP_SUFFIX = '''
    closed = httpCloseClient client
    "${result}; client close=${closed}"
request () =
    client = httpCreateClient ("http://127.0.0.1:18898", 1000)
    match client > 0
        true => runRequest client
        false => "client error: ${client}"
serve server =
    listening = httpListen (server, reply)
    result = match listening == 0
        true => request ()
        false => "listen error: ${listening}"
    stopped = httpStopServer server
    "${result}; server stop=${stopped}"
server = httpCreateServer (18898, "127.0.0.1")
match server > 0
    true => print (serve server)
    false => print "server error: ${server}"'''

HTTP_RESPONSE_HELPER = '''inspect response =
    summary = "status=${httpResponseStatus response}; body=${httpResponseBody response}; header=${httpResponseHeader (response, "X-Demo")}"
    released = httpResponseFree response
    "${summary}; released=${released}"
'''

HTTP_RESPONSE_ACTION = '''match httpGetResponse (client, "/", "")
        Success value => inspect value
        Error message => message'''

HTTP_CLIENT = '''client = httpCreateClient ("http://127.0.0.1:18898", 1000)
match client > 0
    true => print "closed: ${httpCloseClient client}"
    false => print "client error: ${client}"'''

WEBSOCKET = '''communicate (server, connection) =
    sent = websocketSend (connection, "Hello")
    broadcast = websocketServerBroadcast (server, "Welcome")
    closed = websocketClose connection
    "sent=${sent}; broadcast=${broadcast}; closed=${closed}"
exchange server =
    connection = websocketConnect "ws://127.0.0.1:18899/chat"
    match connection > 0
        true => communicate (server, connection)
        false => "connection error: ${connection}"
server = websocketCreateServer (18899, "127.0.0.1", "/chat")
listening = match server > 0
    true => websocketServerListen server
    false => server
match listening == 0
    true => print (exchange server)
    false => print "server error: ${listening}"'''

WEBSOCKET_KEEP_ALIVE = '''// A long-running server; Ctrl+C stops the keep-alive loop.
server = websocketCreateServer (18899, "127.0.0.1", "/chat")
listening = match server > 0
    true => websocketServerListen server
    false => server
print "listen status: ${listening}"
match listening == 0
    true => websocketKeepAlive ()
    false => print "server could not start"'''

SIGNAL = '''static effect Signal T
    read : Unit => T
type Count =
    value : int
readCount () = (perform Signal<Count>.read ()).value
counterLabel () = "count=${readCount ()}"
cached =
    handle static Signal<Count>
        read => Count(value = 1)
    signalSnapshot counterLabel
updated =
    handle static Signal<Count>
        read => Count(value = 2)
    signalRefresh (cached, signalDependencies readCount, counterLabel)
print "${updated.value}; revision=${updated.revision}; ${join (signalDependencies counterLabel, ",")}"'''

GPU_MATMUL = '''left = toGpu [1.0, 2.0, 3.0, 4.0]
right = toGpu [5.0, 6.0, 7.0, 8.0]
match gpuMatMul (left, right, 2, 2, 2)
    Success value => print (gpuGet (value, 0) ?: 0.0)  // 19.0
    Error message => print message'''

FIBER = '''work () = "done"
worker = spawn work ()
result = await worker
print "${result}; finished=${fiberDone worker}"'''


def http_example(action, helpers=""):
    return f"{helpers}{HTTP_PREFIX}{action}{HTTP_SUFFIX}"


def pages(names, source):
    return dict.fromkeys((f"functions/{name.lower()}.md" for name in names), source)


PROCESS = 'onEvent (_pid, kind, data) = print "${kind}: ${trim data}"\nmatch spawnProcess "echo hello" onEvent\n    Success value =>\n        exitCode = awaitProcess value\n        cleanupProcess value\n        print "exit: ${exitCode}"\n    Error message => print "Process failed: ${message}"'

HAND_TWINS = {'functions/mapget.md': 'match mapGet (["alice" => 42], "alice")\n'
                        '    Success value => print value\n'
                        '    Error message => print message',
 'functions/floattoint.md': 'describe x = match floatToInt x\n'
                            '    Success value => toString value\n'
                            '    Error message => message\n'
                            'print "${describe (-2.9)}\\n${describe (sqrt (-1.0))}"',
 'functions/indexof.md': 'match indexOf ("foo=bar", "=")\n'
                         '    Success value => print value\n'
                         '    Error message => print message',
 'functions/structuralequals.md': 'type Route =\n'
                                  '    tab : string\n'
                                  '    depth : int\n'
                                  'same = structuralEquals (Route(tab = "home", depth = 1)) (Route(tab = '
                                  '"home", depth = 1))\n'
                                  'differ = structuralEquals [1, 2] [1, 2, 3]',
 'functions/yield.md': 'yield',
 'keywords/false.md': 'isComplete = false\n'
                      'match isComplete\n'
                      '    false => print "Not done yet"\n'
                      '    true => print "done"',
 'keywords/true.md': 'isReady = true\nmatch isReady\n    true => print "Ready!"\n    false => print "Wait"',
 'keywords/type.md': 'type UserId =\n'
                     '    int\n'
                     '\n'
                     'type Status =\n'
                     '    Active\n'
                     '    Inactive\n'
                     '\n'
                     'type User =\n'
                     '    name : string\n'
                     '    age : int'}

HAND_TWINS.update({
    **pages(["__jsonDecodeOpen", "__jsonDecodeClose", "__jsonNodeKind", "__jsonNodeFirst", "__jsonNodeEnd", "__jsonNodeKey", "__jsonNodeText", "__jsonNodeInt", "__jsonNodeFloat"], JSON_DECODE),
    **pages(["__jsonWriterOpen", "__jsonWriteRaw", "__jsonWriteString", "__jsonWriteInt", "__jsonWriteFloat", "__jsonWriterClose"], JSON_ENCODE),
    **pages(["jsonParse", "jsonGet", "jsonLength", "jsonFree"], JSON_DOCUMENT),
    **pages(["spawnProcess", "awaitProcess", "cleanupProcess"], PROCESS),
    **pages(["httpCreateClient", "httpCloseClient"], HTTP_CLIENT),
    **pages(["httpGet", "httpCreateServer", "httpListen", "httpStopServer"], http_example('toString (httpGet (client, "/", ""))')),
    "functions/httppost.md": http_example('toString (httpPost (client, "/", "hello", "Content-Type: text/plain"))'),
    "functions/httpput.md": http_example('toString (httpPut (client, "/", "updated", "Content-Type: text/plain"))'),
    "functions/httpdelete.md": http_example('toString (httpDelete (client, "/", ""))'),
    **pages(["httpGetResponse", "httpResponseStatus", "httpResponseBody", "httpResponseHeader", "httpResponseFree"], http_example(HTTP_RESPONSE_ACTION, HTTP_RESPONSE_HELPER)),
    **pages(["websocketCreateServer", "websocketServerListen", "websocketServerBroadcast", "websocketConnect", "websocketSend", "websocketClose"], WEBSOCKET),
    "functions/websocketkeepalive.md": WEBSOCKET_KEEP_ALIVE,
    **pages(["signalDependencies", "signalSnapshot", "signalRefresh"], SIGNAL),
    "functions/gpumatmul.md": GPU_MATMUL,
    **pages(["await", "fiberDone"], FIBER),
})
