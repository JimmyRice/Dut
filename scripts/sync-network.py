#!/usr/bin/env python3
"""Compare MTR open data with the network compiled into Dut, and regenerate
the compiled station table from it.

Dut logs a warning when open data disagrees with the compiled network, for
example after a new station opens. Run this script against a running Dut to
see what differs:

    cargo run
    python3 scripts/sync-network.py

It reads the cleaned station list from GET /api/data/stations and the
compiled network from GET /api/lines, so every cleaning rule lives once, in
Rust. With --write it rewrites the STATIONS table in
crates/dut-core/src/domain/network/station.rs between its GENERATED markers:
names follow open data, and stations open data leaves out, such as
Racecourse, are kept as they are.

Line layouts, termini, and branches need a person's judgement, so for lines
whose stations differ it only prints the published routes as `codes![..]`
to copy from. Afterwards run `cargo fmt && cargo test` and review the diff.

Exits 0 when nothing differs, 1 when something does, and 2 when Dut cannot
be read. Needs only the Python 3 standard library.
"""

import argparse
import json
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parent.parent
STATION_TABLE = REPOSITORY / "crates/dut-core/src/domain/network/station.rs"
BEGIN = "// BEGIN GENERATED STATIONS\n"
END = "// END GENERATED STATIONS\n"
# Matches one table entry, with any comment lines just above it.
ENTRY = re.compile(
    r'(?P<comments>(?:[ \t]*//[^\n]*\n)*)[ \t]*Station::new\("(?P<code>[A-Z]{3})"'
)


def main() -> int:
    arguments = parse_arguments()
    try:
        published = read(arguments.base_url, "/api/data/stations")
        compiled = read(arguments.base_url, "/api/lines")
    except (urllib.error.URLError, OSError, ValueError) as error:
        print(f"Could not read Dut at {arguments.base_url}: {error}", file=sys.stderr)
        print("Start it first with `cargo run`.", file=sys.stderr)
        return 2

    published_names = {s["code"]: s["name"] for s in published["stations"]}
    compiled_names = {
        station["code"]: station["name"]
        for line in compiled["lines"]
        for station in line["stations"]
    }

    differs = report_stations(published_names, compiled_names)
    differs |= report_lines(published, compiled, published_names)
    if not differs:
        print("Open data agrees with the compiled network.")

    if arguments.write:
        write_station_table(published_names, compiled_names)
    return 1 if differs else 0


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Compare MTR open data with Dut's compiled network."
    )
    parser.add_argument(
        "--base-url",
        default="http://127.0.0.1:3000",
        help="where Dut is running (default: %(default)s)",
    )
    parser.add_argument(
        "--write",
        action="store_true",
        help="rewrite the STATIONS table in station.rs from open data",
    )
    return parser.parse_args()


def read(base_url: str, path: str) -> dict:
    request = urllib.request.Request(
        base_url.rstrip("/") + path, headers={"Accept": "application/json"}
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def report_stations(published: dict, compiled: dict) -> bool:
    """Prints new, renamed, and unpublished stations. Returns whether any
    station is new or renamed; unpublished stations are expected."""
    new = sorted(set(published) - set(compiled))
    unpublished = sorted(set(compiled) - set(published))
    renamed = sorted(
        code for code in set(published) & set(compiled) if published[code] != compiled[code]
    )

    for code in new:
        print(f"New station {code}: {describe(published[code])}")
    for code in renamed:
        print(f"Renamed {code}: {describe(compiled[code])} -> {describe(published[code])}")
    for code in unpublished:
        print(f"Not in open data, kept: {code} {describe(compiled[code])}")
    return bool(new or renamed)


def report_lines(published: dict, compiled: dict, published_names: dict) -> bool:
    """Prints each line whose stations differ, with its published routes.
    Stations open data leaves out everywhere are not counted as differences,
    matching Dut's own drift check."""
    routes_by_line = {line["code"]: line["routes"] for line in published["lines"]}
    differs = False
    for line in compiled["lines"]:
        code = line["code"]
        compiled_stations = {
            station["code"]
            for station in line["stations"]
            if station["code"] in published_names
        }
        routes = routes_by_line.get(code, [])
        published_stations = {station for route in routes for station in route["stations"]}
        added = sorted(published_stations - compiled_stations)
        removed = sorted(compiled_stations - published_stations)
        if not added and not removed:
            continue

        differs = True
        print(f"\n{code} differs: added {added or 'none'}, removed {removed or 'none'}")
        print("Published routes, to update `stations`, `layout`, and `termini` in line.rs:")
        for route in routes:
            towards = route["towards"]["code"] if route["towards"] else "?"
            codes = ", ".join(f'"{station}"' for station in route["stations"])
            print(f"  {route['direction']} towards {towards}: codes![{codes}]")
    return differs


def write_station_table(published: dict, compiled: dict) -> None:
    source = STATION_TABLE.read_text(encoding="utf-8")
    start = source.index(BEGIN) + len(BEGIN)
    end = source.index(END)
    comments = {
        match["code"]: match["comments"] for match in ENTRY.finditer(source[start:end])
    }

    entries = []
    for code in sorted(set(published) | set(compiled)):
        name = published.get(code) or compiled[code]
        entry = f"    Station::new({rust_string(code)}, {rust_string(name['en'])}, {rust_string(name['tc'])}),\n"
        entries.append(comments.get(code, "") + entry)
    table = "const STATIONS: &[Station] = &[\n" + "".join(entries) + "];\n"

    STATION_TABLE.write_text(source[:start] + table + source[end:], encoding="utf-8")
    print(f"\nWrote {len(entries)} stations to {STATION_TABLE.relative_to(REPOSITORY)}.")
    print("Next: cargo fmt && cargo test, then review the diff.")


def rust_string(text: str) -> str:
    """A Rust string literal: JSON's escaping of quotes and backslashes is
    valid Rust, and non-ASCII characters are kept as they are."""
    return json.dumps(text, ensure_ascii=False)


def describe(name: dict) -> str:
    return f"{name['en']} / {name['tc']}"


if __name__ == "__main__":
    sys.exit(main())
