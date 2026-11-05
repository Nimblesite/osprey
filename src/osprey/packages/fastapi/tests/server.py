# osprey: flavor=python
from fastapi import Applications as A
from fastapi_tests import Fixture


def serve_on(port):
    match A.serve(port, "127.0.0.1", Fixture.application(port)):
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
            return serve_on(port)
        case Error(message=message):
            return print(f"port error: {message}")
