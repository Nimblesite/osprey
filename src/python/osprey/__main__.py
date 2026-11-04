"""Run an Osprey entry inside a Python host, with Python callbacks installed."""

import argparse
from . import build


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", help="Osprey source file or project")
    parser.add_argument("--compiler", help="Osprey compiler executable")
    arguments = parser.parse_args()
    build(arguments.source, compiler=arguments.compiler)


if __name__ == "__main__":
    main()
