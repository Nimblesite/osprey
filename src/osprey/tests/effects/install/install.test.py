# osprey: flavor=python

# The package manager's file-system effect: one install, a real disk and
# three test doubles. The Playground opens on this program in every flavor.
@effect
class Files:
    def read(path: str) -> Result[str, Error]: ...
    def write(path: str, text: str) -> Result[int, Error]: ...


def commit_lock(previous, lock):
    match perform(Files.write("osprey.lock", lock)):
        case Success():
            return "installed"
        case Error(message=message):
            match perform(Files.write("osprey.toml", previous)):
                case Success():
                    return f"rolled back: {message}"
                case Error():
                    return f"manifest left changed: {message}"


def install(manifest, lock):
    match perform(Files.read("osprey.toml")):
        case Error(message=message):
            return f"no manifest: {message}"
        case Success(value=previous):
            match perform(Files.write("osprey.toml", manifest)):
                case Error(message=message):
                    return f"manifest not written: {message}"
                case Success():
                    return commit_lock(previous, lock)


# Production passes the real disk (the Playground sandbox has none):
# @handler(Files) class disk: read returns readFile(path), write returns writeFile(path, text)


# Test doubles: canned disks, no files touched.
@handler(Files)
class healthy_disk:
    def read(_path):
        return Success(value="version = 1")

    def write(_path, text):
        return Success(value=byteLength(text))


@handler(Files)
class full_disk:
    def read(_path):
        return Success(value="version = 1")

    def write(path, text):
        return Error(message="disk full") if path == "osprey.lock" else Success(value=byteLength(text))


@handler(Files)
class read_only_disk:
    def read(_path):
        return Success(value="version = 1")

    def write(_path, _text):
        return Error(message="read-only")


def upgrade():
    return install("version = 2", "locked")


def main():
    test("a healthy disk installs", lambda: expect(healthy_disk(upgrade), "installed"))
    test("a full disk rolls the manifest back", lambda: expect(full_disk(upgrade), "rolled back: disk full"))
    with read_only_disk:
        test("a read-only disk changes nothing", lambda: expect(upgrade(), "manifest not written: read-only"))
