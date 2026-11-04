# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Applications
"""Serve typed Python flavor handlers through Osprey HTTP. [FASTAPI-HTTP-SERVE]"""
from fastapi import Models as M
from fastapi import Responses as R
from fastapi import Routing

__all__ = ["serve"]


def serve(port, host, routes):
    match httpCreateServer(port, host):
        case Error(message=message):
            return Error(message=message)
        case Success(value=server):
            listening = httpListen(server, lambda method, url, headers, body: R.native(Routing.dispatch(routes, M.Request(method=method, url=url, headers=headers, body=body))))
            match listening:
                case Success():
                    return Success(value=server)
                case Error(message=message):
                    match httpStopServer(server):
                        case Success():
                            return Error(message=message)
                        case Error(message=cleanup):
                            return Error(message=f"{message}; listener cleanup failed: {cleanup}")
