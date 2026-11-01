# osprey: flavor=python
# osprey: namespace=chromecast
# osprey: module=Media
"""Serving the file a device will fetch.

A Cast device plays a URL, so the sender serves the local file over HTTP on
the interface that routes to the device ([FFI-FILE-SERVER]) through the
`Wire` effect, and names the media after the file. A receiver shows only
still artwork for audio, so when the sender can render it the file is cast
as a visualiser video instead: an HLS stream whose picture moves with the
music ([FFI-MEDIA-RENDER]). Without a renderer the audio is cast as it is.
"""

from chromecast import Wire

__all__ = ["Served", "NOTHING", "title", "media_url", "unserve"]


class Served:
    """The media's URL, the port of the server behind it, the render folder
    ("" when the audio is served as it is) and the content type to LOAD."""
    url: str
    port: int
    folder: str
    content_type: str


NOTHING = Served(url="", port=0, folder="", content_type="")
"""Nothing served, as an idle player holds."""


def base_name(path):
    parts = unwrap_or(split(path, "/"), [path])
    return unwrap_or(listGet(parts, len(parts) - 1), path)


def leading_dots(s):
    """How many `.` characters a name starts with."""
    return fold(range(0, length(s)), 0, lambda n, i: n + 1 if n == i and take(drop(s, i), 1) == "." else n)


def title(path):
    """The media title: the file's base name without its extension, as
    `os.path.splitext` cuts it (leading dots are not an extension)."""
    name = base_name(path)
    dots = leading_dots(name)
    parts = unwrap_or(split(drop(name, dots), "."), [])
    if len(parts) <= 1:
        return name
    else:
        kept = toList(map(range(0, len(parts) - 1), lambda i: unwrap_or(listGet(parts, i), "")))
        return take(name, dots) + join(kept, ".")


def served(address, port, folder):
    """The record for a server on `port`: the HLS playlist of a render, else the audio."""
    if folder != "":
        return Served(url=f"http://{address}:{port}/media.m3u8", port=port, folder=folder, content_type="application/x-mpegURL")
    else:
        return Served(url=f"http://{address}:{port}/media", port=port, folder="", content_type="audio/mpeg")


def media_url(path, host) -> Result[Served, str]:
    """Serves `path` on the interface that routes to `host`, as a visualiser
    video when it can be rendered, and returns what was served."""
    address = perform(Wire.Wire.route(host))
    folder = perform(Wire.Wire.render(path)) if address != "" else ""
    port = perform(Wire.Wire.serve(folder if folder != "" else path, "audio/mpeg", address)) if address != "" else -1
    if address == "":
        return Error(message=f"{host}: no route to the device")
    elif port < 1:
        _dropped = perform(Wire.Wire.unrender(folder)) if folder != "" else 0
        return Error(message=f"{path}: cannot serve the file ({port})")
    else:
        return Success(value=served(address, port, folder))


def unserve(media):
    """Stops serving `media` and its render; 0, or -1 if no server runs on its port."""
    _dropped = perform(Wire.Wire.unrender(media.folder)) if media.folder != "" else 0
    return perform(Wire.Wire.unserve(media.port))
