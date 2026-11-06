"""One bounded native heap per worker, with an all-records-or-rollback transaction."""

import sys

import osprey
from .session import Session
from .store import connect


def run(database, library_path, start, count):
    connection = connect(database)
    try:
        library = osprey.load(library_path)
        session = Session(connection, library, start, count)
        handle = library.keep(session)
        try:
            library.audit_batch(handle, start, count)
            return session.finish()
        finally:
            library.release(handle)
    finally:
        connection.close()


def main():
    try:
        database, library, start, count = sys.argv[1:]
        run(database, library, int(start), int(count))
        return 0
    except Exception as error:
        print(f"Audit worker failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
