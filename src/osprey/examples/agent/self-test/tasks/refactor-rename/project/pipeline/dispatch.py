from .core import compact

HANDLERS = {"compact": compact}


def run(action, text):
    return HANDLERS[action](text)
