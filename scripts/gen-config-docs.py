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
AREA_TITLES = {
    "agents": "Coding agents",
    "evidence": "Evidence sources",
    "notifications": "Notification providers",
    "reproduction": "Reproduction targets",
    "ticketing": "Ticketing providers",
}

# How each area's secret fields are resolved, under its table.
SECRET_NOTE = (
    "A secret field is read from the secrets store under the name shown, then "
    "from the config, then from the environment variable of that name"
)
AREA_NOTES = {
    "evidence": (
        f"{SECRET_NOTE}; a board's own source reads its config value first. Every source also "
        "takes `label`, `areas` and `notes` ([evidence.md](evidence.md#configuring-sources)), "
        "left out of the table."
    ),
    "notifications": f"{SECRET_NOTE}; a board's own target reads its config value first.",
    "ticketing": f"{SECRET_NOTE}. Each field is a key of the `ticketing` section.",
    "reproduction": (
        "Each field is a key of the `app_runs` section; a remote target has no "
        "global settings, only each repository's `[app.remote]`. A board picks "
        "its target with `reproduction.target`."
    ),
}


# Fields every provider of an area shares, documented in the area's note
# instead of under each provider.
COMMON_FIELDS = {
    "evidence": {"label", "areas", "notes"},
}


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


# How each agent's sessions get MCP servers, by the descriptor's `mcp`.
MCP_WIRING = {
    "per_session": "Per session, on its command line, on top of its own",
    "agent_config": "Only from its own config (**Settings → MCP Servers**)",
    "unsupported": "None",
}


def render_agents(descriptors: list) -> list:
    """The coding agents: what each needs and how it signs in, not fields;
    then the `agents` section's own settings, which are fields."""
    agents = [descriptor for descriptor in descriptors if descriptor.get("agent")]
    settings = [descriptor for descriptor in descriptors if not descriptor.get("agent")]
    lines = [
        "",
        f"### {AREA_TITLES['agents']}",
        "",
        "Generated from the agents' descriptors. `executor_profile` and every "
        "`agents.stages` entry name one of these as `executor`, with an optional "
        "`variant`; **Settings → Agents** probes each with its command and "
        "`--version`.",
        "",
        "| `executor` | Command | Signs in | MCP servers | Variants |",
        "| --- | --- | --- | --- | --- |",
    ]
    for descriptor in agents:
        agent = descriptor["agent"]
        name = f"`{descriptor['kind']}` ({cell(descriptor['display_name'])})"
        command = f"`{agent['command']}`" if agent["command"] else "built in"
        roles = {
            agent.get("economy_variant"): " (economy)",
            agent.get("guarded_variant"): " (guarded)",
        }
        variants = ", ".join(
            f"`{variant}`" + roles.get(variant, "") for variant in agent["variants"]
        )
        lines.append(
            f"| {name} | {command} | {cell(agent['sign_in'])} | "
            f"{MCP_WIRING[agent['mcp']]} | {variants} |"
        )
    fields = [field for descriptor in settings for field in descriptor["fields"]]
    if fields:
        lines += [
            "",
            "The `agents` section's other settings, from the same descriptors; each is "
            "also a key of a board's `agents`, which wins over the global one.",
            "",
            "| Key | What it is |",
            "| --- | --- |",
        ]
        for field in fields:
            lines.append(
                f"| `agents.{field['key']}` | **{cell(field['label'])}**. {cell(field['help'])} |"
            )
    return lines


def render(descriptors: dict) -> str:
    lines = [BEGIN]
    for area in sorted(descriptors):
        if area == "agents":
            lines += render_agents(descriptors[area])
            continue
        title = AREA_TITLES.get(area, area.replace("_", " ").capitalize())
        lines += [
            "",
            f"### {title}",
            "",
            "Generated from the providers' descriptors. "
            + AREA_NOTES.get(area, f"{SECRET_NOTE}."),
            "",
            "| `kind` | Field | Required | Secret name | What it is |",
            "| --- | --- | --- | --- | --- |",
        ]
        for provider in descriptors[area]:
            kind = f"`{provider['kind']}` ({cell(provider['display_name'])})"
            if not provider.get("available", True):
                kind += " — not implemented"
            lines.append(f"| {kind} | | | | {cell(provider['summary'])} |")
            # `secrets` lists one store name per secret field, in field order.
            secret_names = iter(provider["secrets"])
            for field in provider["fields"]:
                secret = ""
                if field["kind"]["type"] == "secret":
                    secret = f"`{next(secret_names)}`"
                if field["key"] in COMMON_FIELDS.get(area, set()):
                    continue
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
