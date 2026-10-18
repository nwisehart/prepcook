#!/usr/bin/env python3
"""Regenerates the integration provider tables in docs/configuration.md.

The tables are generated from the providers' own descriptors, read through the
binary (`prepcook integrations describe --json`), so a new provider or field is
documented by the same code that defines it. Everything outside the marked
block is written by hand and left alone.

Usage:
  scripts/gen-config-docs.py            rewrite the block
  scripts/gen-config-docs.py --check    exit 1 when the committed block is stale

The binary is $PREPCOOK_BIN when set, else `cargo run --bin prepcook`.
"""

import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DOC = ROOT / "docs" / "configuration.md"
BEGIN = "<!-- BEGIN GENERATED: integrations (scripts/gen-config-docs.py) -->"
END = "<!-- END GENERATED: integrations -->"

# Section titles for areas whose name does not read well capitalised.
AREA_TITLES = {"notifications": "Notification providers"}


def describe() -> dict:
    binary = os.environ.get("PREPCOOK_BIN")
    if binary:
        command = [binary, "integrations", "describe", "--json"]
    else:
        command = [
            "cargo",
            "run",
            "--quiet",
            "--bin",
            "prepcook",
            "--",
            "integrations",
            "describe",
            "--json",
        ]
    output = subprocess.run(
        command, cwd=ROOT, check=True, capture_output=True, text=True
    ).stdout
    return json.loads(output)


def cell(text: str) -> str:
    return " ".join(text.split()).replace("|", "\\|")


def render(descriptors: dict) -> str:
    lines = [BEGIN]
    for area in sorted(descriptors):
        title = AREA_TITLES.get(area, area.replace("_", " ").capitalize())
        lines += [
            "",
            f"### {title}",
            "",
            "Generated from the providers' descriptors. A secret field is read from "
            "the secrets store under the name shown, then from the config, then from "
            "the environment variable of that name; a board's own target reads its "
            "config value first.",
            "",
            "| `kind` | Field | Required | Secret name | What it is |",
            "| --- | --- | --- | --- | --- |",
        ]
        for provider in descriptors[area]:
            kind = f"`{provider['kind']}` ({cell(provider['display_name'])})"
            lines.append(f"| {kind} | | | | {cell(provider['summary'])} |")
            # `secrets` lists one store name per secret field, in field order.
            secret_names = iter(provider["secrets"])
            for field in provider["fields"]:
                secret = ""
                if field["kind"]["type"] == "secret":
                    secret = f"`{next(secret_names)}`"
                required = "yes" if field["required"] else "no"
                lines.append(
                    f"| | `{field['key']}` | {required} | {secret} | "
                    f"**{cell(field['label'])}**. {cell(field['help'])} |"
                )
    lines += ["", END]
    return "\n".join(lines)


def main() -> int:
    check = "--check" in sys.argv[1:]
    text = DOC.read_text()
    if BEGIN not in text or END not in text:
        print(f"{DOC} has no generated block ({BEGIN} … {END})", file=sys.stderr)
        return 1
    head, rest = text.split(BEGIN, 1)
    _, tail = rest.split(END, 1)
    updated = head + render(describe()) + tail
    if check:
        if updated != text:
            print(
                f"{DOC.relative_to(ROOT)} is stale; run scripts/gen-config-docs.py",
                file=sys.stderr,
            )
            return 1
        print(f"{DOC.relative_to(ROOT)} is up to date.")
        return 0
    DOC.write_text(updated)
    return 0


if __name__ == "__main__":
    sys.exit(main())
