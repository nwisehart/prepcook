"""Reads the inputs the toolkit is generated from, all out of this checkout.

The playbooks are Rust raw string constants in
`crates/services/src/services/config/playbooks.rs`. `default_playbooks()`
maps each key to a constant (`("bug:planning".to_string(),
BUG_PLANNING.to_string())`, or a key constant such as
`REVIEWING_PLAYBOOK_KEY`); this module reads that map, then the `r#"…"#`
body of each constant it names. Nothing else in the file is read, so the
gate texts and the tests below them never leak into a playbook. The one
shared passage every ticket-reading playbook carries, `{{ticket_is_data}}`,
is filled here from `TICKET_IS_DATA` in the same file, as the orchestrator
fills it at launch.

`source_sha()` names the commit the inputs were last changed in, for the
note at the top of every generated file.
"""

from __future__ import annotations

import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
PLAYBOOKS_FILE = "crates/services/src/services/config/playbooks.rs"
SKILLS_DIR = "assets/agent-skills"
DEFAULT_MANIFEST = "docs/examples/scripts.toml"

FUNCTION_HEAD = "pub fn default_playbooks() -> BTreeMap<String, String> {"
FUNCTION_TAIL = "\n}"

# `("bug:planning".to_string(), BUG_PLANNING.to_string())` or
# `(REVIEWING_PLAYBOOK_KEY.to_string(), REVIEWING.to_string())`, across lines.
PAIR = re.compile(r'\(\s*("[a-z:_]+"|[A-Z_]+)\.to_string\(\),\s*([A-Z_]+)\.to_string\(\),?\s*\)')
KEY_CONST = re.compile(r'pub const ([A-Z_]+): &str = "([^"]+)";')
RAW_CONST = re.compile(r'const ([A-Z_]+): &str = r#"(.*?)"#;', re.S)
# `{{ticket_is_data}}` and the constant that fills it.
SHARED_PASSAGES = {"{{ticket_is_data}}": "TICKET_IS_DATA"}


def parse(source: str) -> dict[str, str]:
    """Playbook key → text, from the source of playbooks.rs."""
    start = source.index(FUNCTION_HEAD)
    end = source.index(FUNCTION_TAIL, start)
    keys = dict(KEY_CONST.findall(source))
    texts = dict(RAW_CONST.findall(source))
    playbooks = {}
    for key, const in PAIR.findall(source[start:end]):
        name = key.strip('"') if key.startswith('"') else keys[key]
        text = texts[const]
        for placeholder, passage in SHARED_PASSAGES.items():
            text = text.replace(placeholder, texts[passage])
        playbooks[name] = text
    if not playbooks:
        raise ValueError(f"no playbooks found under default_playbooks() in {PLAYBOOKS_FILE}")
    return playbooks


def load(repo: Path = REPO) -> dict[str, str]:
    """The playbooks as this checkout has them."""
    return parse((repo / PLAYBOOKS_FILE).read_text())


def load_skill(name: str, file: str = "SKILL.md", repo: Path = REPO) -> str:
    """One file of one of the agent skills Prepcook ships to its sessions."""
    return (repo / SKILLS_DIR / name / file).read_text()


def source_sha(paths: list[str], repo: Path = REPO) -> str:
    """The short SHA of the last commit that touched `paths`.

    Uncommitted changes are not marked: a playbook edit is regenerated and
    committed in one go, so the inputs are always dirty at generation time,
    and a suffix saying so would be committed into every skill."""

    def git(*args: str) -> str:
        return subprocess.run(
            ["git", "-C", str(repo), *args], capture_output=True, text=True, check=True
        ).stdout.strip()

    try:
        return git("log", "-1", "--format=%h", "--", *paths) or git("rev-parse", "--short", "HEAD")
    except (OSError, subprocess.CalledProcessError):
        return "unknown"
