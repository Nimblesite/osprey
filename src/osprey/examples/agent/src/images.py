# osprey: flavor=python
# osprey: namespace=agent
# osprey: module=Images
"""Pasted images: the host saves a picture in one folder and leaves `[image: name.png]`
in the words. Only a bare `.png` name counts, so words pasted from a log that look like
a marker can never read a file elsewhere and send it to a provider. Conversations keep
the name; the bytes join each request as it goes. Pure, and tested without a window.
"""
from pydantic_ai import JsonUtil as J

__all__ = ["folder_of", "names_in", "read_arguments", "with_images"]

OPENING = "[image: "
SUFFIX = ".png"
LOST = "(A pasted image is no longer available.)"
READ_SCRIPT = 'cd "$1" && shift; for f; do /usr/bin/base64 < "$f" 2>/dev/null | /usr/bin/tr -d "\\n"; echo; done'


def folder_of(home):
    """Where the host keeps pasted images.

    # Examples
    ```osprey
    from agent import Images

    print(Images.folder_of("/Users/ada"))
    ```
    ```output
    /Users/ada/Library/Application Support/Osprey/pasted
    ```
    """
    return f"{home}/Library/Application Support/Osprey/pasted"


def valid_name(name):
    """A bare `.png` name: nothing that climbs out of the folder."""
    return len(name) > len(SUFFIX) and endsWith(name, SUFFIX) and not contains(name, "/") and not contains(name, "\\") and not startsWith(name, ".")


def marker_name(piece):
    """The name at the start of the text after an opening, empty unless it closes and is valid."""
    parts = unwrap_or(split(piece, "]"), [piece])
    name = unwrap_or(listGet(parts, 0), "")
    return name if len(parts) > 1 and valid_name(name) else ""


def tail_of(parts):
    return fold(range(len(parts)), [], lambda acc, index: acc if index == 0 else listAppend(acc, unwrap_or(listGet(parts, index), "")))


def pieces_after_opening(text):
    return tail_of(unwrap_or(split(text, OPENING), [text]))


def text_names(text):
    return filterList(mapList(pieces_after_opening(text), marker_name), lambda name: name != "")


def without_markers(text):
    """`text` with each valid marker taken out; anything else that looks like one stays as written."""
    parts = unwrap_or(split(text, OPENING), [text])
    return trim(foldList(tail_of(parts), unwrap_or(listGet(parts, 0), ""), lambda acc, piece: acc + after_marker(piece)))


def after_marker(piece):
    name = marker_name(piece)
    return OPENING + piece if name == "" else drop(piece, len(name) + 1)


def is_user(message):
    return J.text_at(message, "role", "") == "user"


def user_texts(body):
    return mapList(filterList(J.list_at(body, "messages"), is_user), lambda message: J.text_at(message, "content", ""))


def names_in(body_text):
    """Every pasted image the people's words in this request name, in order.

    # Examples
    ```osprey
    from agent import Images

    body = '{"messages":[{"role":"user","content":"see [image: shot.png] then [image: ../shot.png]"},{"role":"assistant","content":"[image: reply.png]"}]}'
    print(f"{Images.names_in(body)} {Images.names_in('not json')}")
    ```
    ```output
    ["shot.png"] []
    ```
    """
    match J.decode(body_text):
        case Success(value=body):
            return foldList(user_texts(body), [], lambda acc, text: listConcat(acc, text_names(text)))
        case Error():
            return []


def read_arguments(home, names):
    """`/bin/sh` arguments printing each image as one base64 line, empty when it is gone.

    # Examples
    ```osprey
    from agent import Images

    args = Images.read_arguments("/Users/ada", ["shot.png"])
    print(f"{len(Images.read_arguments('/Users/ada', []))} {args[2]} {args[3]} {args[4]}")
    ```
    ```output
    5 sh /Users/ada/Library/Application Support/Osprey/pasted shot.png
    ```
    """
    return listConcat(["-c", READ_SCRIPT, "sh", folder_of(home)], names)


def image_part(data):
    url = J.member("url", JsonString("data:image/png;base64," + data))
    return J.obj([J.member("type", JsonString("image_url")), J.member("image_url", J.obj([url]))])


def text_part(text):
    return J.obj([J.member("type", JsonString("text")), J.member("text", JsonString(text))])


def data_of(names, lines, name):
    return fold(range(len(names)), "", lambda acc, index: trim(unwrap_or(listGet(lines, index), "")) if acc == "" and unwrap_or(listGet(names, index), "") == name else acc)


def content_of(text, names, lines):
    """The message's content with its images; words alone when it names none."""
    datas = mapList(text_names(text), lambda name: data_of(names, lines, name))
    found = filterList(datas, lambda data: data != "")
    words = without_markers(text)
    said = words if len(found) == len(datas) else trim(words + "\n\n" + LOST)
    return JsonString(said) if len(found) == 0 else JsonArray(listPrepend(mapList(found, image_part), text_part(said)))


def with_content(message, names, lines):
    text = J.text_at(message, "content", "")
    plain = not is_user(message) or len(text_names(text)) == 0
    return message if plain else J.obj(mapList(J.members_of(message), lambda m: J.member("content", content_of(text, names, lines)) if J.member_name(m) == "content" else m))


def with_images(body_text, names, output):
    """The request `body_text` with each named image's base64 line from `output` in place of its marker.

    # Examples
    ```osprey
    from agent import Images

    body = '{"messages":[{"role":"user","content":"look [image: shot.png]"}]}'
    kept = Images.with_images("not json", ["shot.png"], "QUJD")
    print(f"{Images.with_images(body, ['shot.png'], 'QUJD')} | {kept}")
    ```
    ```output
    {"messages":[{"role":"user","content":[{"type":"image_url","image_url":{"url":"data:image/png;base64,QUJD"}},{"type":"text","text":"look"}]}]} | not json
    ```
    """
    lines = unwrap_or(split(output, "\n"), [])
    match J.decode(body_text):
        case Success(value=body):
            return J.encode(J.obj(mapList(J.members_of(body), lambda m: J.member("messages", JsonArray(mapList(J.list_at(body, "messages"), lambda message: with_content(message, names, lines)))) if J.member_name(m) == "messages" else m)))
        case Error():
            return body_text
