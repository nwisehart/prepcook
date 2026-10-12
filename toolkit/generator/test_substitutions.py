"""The substitution table against the playbooks in this checkout.

    python3 -m unittest discover -s toolkit/generator -v

Each check names a way the toolkit drifts from the orchestrator:

  every rule still matches — a playbook edit that rewords a sentence the
  table rewrites would otherwise generate a skill that still talks about the
  card, the gate or `app_restart`;

  nothing the orchestrator alone understands survives into a skill, and no
  company's vocabulary does either;

  the table is idempotent — applying it to its own output changes nothing,
  so regenerating is safe to repeat;

  the committed skills are what generate.py would write.
"""

from __future__ import annotations

import re
import unittest

import generate
import playbooks
from substitutions import COPIED_FILES, COPIED_RULES, RULES, SKILLS

# Words that only mean something to an agent the orchestrator launched. A
# generated skill that still says one of these is telling an engineer to do
# something they cannot.
BOARD_ONLY = [
    "update_issue",
    "add_issue_comment",
    "record_review",
    "list_issue_comments",
    "tracker_add_comment",
    "tracker_get_attachments",
    "app_status",
    "app_restart",
    "lavish",
    "end your turn",
    "END YOUR TURN",
    "the turn",
    "planning_",
    "planning.",
    "the card",
    "on the card",
    "push gate",
    "spec_approved",
    "Prep Cook has already",
    "Executing when ready",
    "{{",
    "}}",
]

# One company's vocabulary, from the toolkit this one was ported from. Spelled
# in pieces so a repository-wide search for these words finds only real leaks.
COMPANY_WORDS = [
    "mat" + "ik",
    "M" + "AO",
    "dev " + "tenant",
    "account " + "pool",
    "account" + "_pool",
    "D" + "EP-",
    "./" + "m ",
    "./" + "m`",
    "starter " + "kit",
]
HOST_PAIR = "`gh` or `glab`"


def company_words_in(text: str) -> list[str]:
    found = [w for w in COMPANY_WORDS if w.lower() in text.lower()]
    # `glab` may appear only as half of the pair that names both hosts.
    if "glab" in text.replace(HOST_PAIR, ""):
        found.append("glab outside a " + HOST_PAIR + " pair")
    return found


def replay(rules, texts: dict[str, str], cut_for=None) -> dict[str, int]:
    """Applies `rules` in order to `texts`, cutting a skill's text from its
    playbook the first time a rule targets it, and returns each rule's match
    count against the text it actually saw."""
    counts = {}
    for index, rule in enumerate(rules):
        if rule.target not in texts and cut_for is not None:
            texts[rule.target] = cut_for(rule.target, texts)
        counts[index] = texts[rule.target].count(rule.old)
        texts[rule.target] = texts[rule.target].replace(rule.old, rule.new)
    return counts


class SubstitutionTable(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.source = playbooks.load()
        cls.profile = generate.load_profile(playbooks.REPO / playbooks.DEFAULT_MANIFEST)
        cls.bodies = generate.build(cls.source, cls.profile)
        cls.files = generate.generate(playbooks.REPO / playbooks.DEFAULT_MANIFEST)

    def label(self, rule) -> str:
        return f"[{rule.concern}] {rule.target}: {rule.old[:60]!r}"

    def test_every_rule_has_a_concern_and_a_reason(self):
        concerns = {"template", "environment", "board", "gate", "runner"}
        for rule in RULES + COPIED_RULES:
            self.assertIn(rule.concern, concerns, self.label(rule))
            self.assertTrue(rule.why.strip(), f"{self.label(rule)} has no why")

    def test_every_rule_matches(self):
        # Rules run in order, each against the text the ones before it left,
        # so the check replays that order: a per-skill rule sees the playbook
        # after every playbook-wide rule and the cut.
        def cut_for(name, texts):
            skill = next(s for s in SKILLS if s.name == name)
            return generate.cut(texts[skill.playbook], skill)

        counts = replay(RULES, dict(self.source), cut_for)
        for index, rule in enumerate(RULES):
            if rule.many:
                self.assertGreaterEqual(counts[index], 1, f"{self.label(rule)} no longer matches")
            else:
                self.assertEqual(counts[index], 1, f"{self.label(rule)} matches {counts[index]} times, expected 1")

    def test_every_copied_rule_matches(self):
        texts = {f"{n}/{f}": playbooks.load_skill(n, f) for n, files in COPIED_FILES.items() for f in files}
        counts = replay(COPIED_RULES, texts)
        for index, rule in enumerate(COPIED_RULES):
            self.assertEqual(counts[index], 1, f"{self.label(rule)} matches {counts[index]} times, expected 1")

    def test_every_anchor_is_found_once(self):
        for skill in SKILLS:
            if skill.anchor is None:
                continue
            rewritten = generate.apply(self.source[skill.playbook], RULES, skill.playbook)
            self.assertEqual(
                rewritten.count(skill.anchor), 1,
                f"{skill.name}: anchor {skill.anchor!r} must appear exactly once in {skill.playbook}",
            )

    def test_no_rule_contains_what_it_replaces(self):
        # An append-style rule (new = old + more) re-applies on every run.
        for rule in RULES + COPIED_RULES:
            if rule.new:
                self.assertNotIn(rule.old, rule.new, f"{self.label(rule)}: new text contains the old")

    def test_the_table_is_idempotent(self):
        for skill in SKILLS:
            body = self.bodies[skill.name]
            again = generate.apply(generate.apply(body, RULES, skill.playbook), RULES, skill.name)
            self.assertEqual(body, again, f"{skill.name}: applying the table twice changed the text")
        for name, files in COPIED_FILES.items():
            for file in files:
                once = generate.apply(playbooks.load_skill(name, file), COPIED_RULES, f"{name}/{file}")
                twice = generate.apply(once, COPIED_RULES, f"{name}/{file}")
                self.assertEqual(once, twice, f"{name}/{file}: applying the table twice changed the text")

    def test_the_halves_add_up(self):
        # A cut skill pair covers its playbook: nothing is lost between them.
        for playbook in ("bug:planning", "bug:executing"):
            rewritten = generate.apply(self.source[playbook], RULES, playbook)
            before, after = [s for s in SKILLS if s.playbook == playbook]
            joined = generate.cut(rewritten, before).rstrip() + "\n\n" + generate.cut(rewritten, after)
            self.assertEqual(joined.split(), rewritten.split(), f"{playbook}: the two halves do not cover it")

    def test_nothing_only_the_orchestrator_understands_survives(self):
        for rel, text in self.files.items():
            for word in BOARD_ONLY:
                self.assertNotIn(word, text, f"{rel} still says {word!r}")

    def test_no_company_word_survives(self):
        skills = generate.SKILLS_OUT
        for path in sorted(skills.rglob("*")):
            if path.is_file():
                rel = path.relative_to(skills.parent)
                self.assertEqual(company_words_in(path.read_text()), [], f"{rel}")
        for rel, text in self.files.items():
            self.assertEqual(company_words_in(text), [], f"{rel} (generated)")

    def test_every_product_placeholder_is_filled(self):
        for rel, text in self.files.items():
            self.assertIsNone(re.search(r"\{\{[#/]?(if )?product\.", text), f"{rel} has an unfilled profile value")
        self.assertIn("The product is Pantry.", self.files[generate.Path("skills/prepcook-repro/SKILL.md")])

    def test_the_committed_skills_are_current(self):
        stale = [str(rel) for rel, _, _ in generate.stale_files(self.files)]
        self.assertEqual(stale, [], "run `python3 toolkit/generator/generate.py` and commit toolkit/skills/")


class Profile(unittest.TestCase):
    def test_blocks_follow_the_values(self):
        text = "P {{product.name}}.{{#if product.areas}} A: {{product.areas}}.{{/if}}" \
               "{{#if product.evidence_tools}} {{product.evidence_tools}}{{else}} none{{/if}}"
        values = {"product.name": "X", "product.areas": "", "product.evidence_tools": "logs"}
        self.assertEqual(generate.render_profile(text, values), "P X. logs")
        values = {"product.name": "X", "product.areas": "a, b", "product.evidence_tools": " "}
        self.assertEqual(generate.render_profile(text, values), "P X. A: a, b. none")


if __name__ == "__main__":
    unittest.main()
