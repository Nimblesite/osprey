# osprey: flavor=python
from fastapi import Applications as A
from fastapi import Results as B
from fastapi_tests import Fixture


def server_application(port, mode):
    if mode == "benchmark":
        return Success(value=A.application(Fixture.routes(), []))
    elif listContains(["cors-nap", "cors-restricted", "cors-all"], mode):
        return Success(value=Fixture.cors_application(mode))
    elif mode == "normal":
        return Success(value=Fixture.application(port))
    else:
        return Error(message="service mode must be normal or benchmark")


def serve_on(port, mode):
    match andThen(server_application(port, mode), lambda app: A.serve(port, "127.0.0.1", app)):
        case Success(value=server):
            sleep(60000)
            match httpStopServer(server):
                case Success():
                    return print("stopped")
                case Error(message=message):
                    return print(f"stop error: {message}")
        case Error(message=message):
            return print(f"server error: {message}")


def main():
    match parseInt(input()):
        case Success(value=port):
            return serve_on(port, input())
        case Error(message=message):
            return print(f"port error: {message}")
