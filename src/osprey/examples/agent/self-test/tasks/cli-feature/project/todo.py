import argparse
import json
from pathlib import Path
import sys


def load(path):
    if not path.exists():
        return []
    return json.loads(path.read_text(encoding="utf-8"))


def save(path, tasks):
    path.write_text(json.dumps(tasks, indent=2) + "\n", encoding="utf-8")


def add(args):
    tasks = load(args.file)
    task_id = max((task["id"] for task in tasks), default=0) + 1
    task = {"id": task_id, "text": args.text, "done": False}
    tasks.append(task)
    save(args.file, tasks)
    print(f"added {task_id}: {args.text}")
    return 0


def list_tasks(args):
    for task in load(args.file):
        mark = "x" if task["done"] else " "
        print(f"[{mark}] {task['id']}: {task['text']}")
    return 0


def parser():
    result = argparse.ArgumentParser(prog="todo")
    result.add_argument("--file", type=Path, default=Path("todo.json"))
    commands = result.add_subparsers(dest="command", required=True)
    add_parser = commands.add_parser("add")
    add_parser.add_argument("text")
    add_parser.set_defaults(handler=add)
    list_parser = commands.add_parser("list")
    list_parser.set_defaults(handler=list_tasks)
    return result


def main(argv=None):
    args = parser().parse_args(argv)
    return args.handler(args)


if __name__ == "__main__":
    raise SystemExit(main())
