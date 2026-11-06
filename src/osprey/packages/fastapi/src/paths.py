# osprey: flavor=python
# osprey: namespace=fastapi
# osprey: module=Paths
"""Ordered path matching and greedy captures. [FASTAPI-HTTP-PARAMETERS]"""
from fastapi import Results as B
__all__ = ['bind', 'matches']

def name_of(segment):
    return B.mapped(checkedSub(len(segment), 2), lambda size: take(drop(segment, 1), size))


def parameter(segment):
    return startsWith(segment, '{') and endsWith(segment, '}')

def greedy(segment):
    return parameter(segment) and endsWith(segment, ':path}')

def bind(pattern, path):
    """Match a route pattern to a decoded path and return its named captures."""
    return B.bind(split(pattern, '/'), lambda patterns: B.bind(split(path, '/'), lambda segments: walk(patterns, segments, {})))

def walk(patterns, segments, params):
    match patterns:
        case []:
            return Success(value=params) if len(segments) == 0 else Error(message='Path does not match')
        case [head, *tail]:
            if greedy(head) and len(tail) == 0 and (len(segments) > 0):
                return B.bind(name_of(head), lambda name: B.mapped(indexOf(name, ':'), lambda index: mapSet(params, take(name, index), join(segments, '/'))))
            else:
                return segment(head, tail, segments, params)

def segment(pattern, patterns, segments, params):
    match segments:
        case []:
            return Error(message='Path does not match')
        case [head, *tail]:
            if parameter(pattern):
                return B.bind(name_of(pattern), lambda name: walk(patterns, tail, mapSet(params, name, head))) if head != '' else Error(message='Empty path segment')
            else:
                return walk(patterns, tail, params) if pattern == head else Error(message='Path does not match')

def matches(pattern, path):
    """Whether the pattern matches the complete decoded path."""
    match bind(pattern, path):
        case Success():
            return True
        case Error():
            return False
