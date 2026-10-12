"""The substitution table: what turns a playbook the orchestrator launches on
a card into a skill an engineer runs on their own machine.

Every rule names the text it replaces exactly as `playbooks.rs` spells it, so
a playbook edit that touches that text fails the tests here instead of
silently generating a skill that still says "post it on the card". The
replacement keeps the playbook's voice; only the facts that differ between
the orchestrator and a laptop change.

Five concerns, and every rule belongs to one:

  template     placeholders the orchestrator fills from the card and the
               repository (`{{ticket_key}}`, `{{title}}`, `{{description}}`,
               `{{spec_path}}`, `{{target_branch}}`, `{{pr_noun}}`) → what a
               skill has instead: `$ARGUMENTS`, `<KEY>`, `<target>`, plain words.
               The `{{product.*}}` placeholders are not rules: generate.py
               fills them from a manifest's `[profile]` after the table runs.
  environment  the app Prep Cook stood up for the worktree (`app_status`,
               `app_restart`) → the app the engineer stood up from the same
               `.prepcook/scripts.toml`, with the same `PREPCOOK_*` variables
  board        the card's tools (`update_issue`, `add_issue_comment`,
               `list_issue_comments`, `record_review`, `tracker_add_comment`)
               → the ticket through the tracker's MCP, a file on disk, the
               knowledge store through the `prepcook` MCP, or the next skill
  gate         the review gate, "end your turn", the human-moves-the-ticket
               steps → dropped; the person is at the keyboard
  runner       what the orchestrator's machine does for the agent (reruns the
               scripts before the pull request, kills processes with the
               turn, refuses `.repro/` at the push gate) → what the engineer does

Rules run in order: the playbook-wide rules (quoting the playbook verbatim,
placeholders and all), the template rules last among them, then the cut at
the skill's anchor, then the skill's own rules against the substituted text.
`generate.py` does the applying; `test_substitutions.py` checks every rule
still matches exactly once (or at least once, for the `many` ones) and that
applying the table twice changes nothing.
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class Rule:
    concern: str
    target: str  # a playbook key (before the cut), a skill name (after it), or a copied file
    old: str
    new: str
    why: str
    many: bool = False  # expected exactly once unless set; `many` means one or more


@dataclass(frozen=True)
class Skill:
    name: str
    playbook: str
    # The sentence the playbook is cut at; `side` says which half the skill keeps.
    anchor: str | None
    side: str  # "before", "from", or "all"
    description: str


SKILLS = [
    Skill(
        "prepcook-repro",
        "bug:planning",
        "With the failure in hand, find the mechanism:",
        "before",
        "Reproduce a reported bug against the app running on your machine with a Playwright harness that "
        "fails on the target branch. Usage: /prepcook-repro <KEY>",
    ),
    Skill(
        "prepcook-rca",
        "bug:planning",
        "With the failure in hand, find the mechanism:",
        "from",
        "Write the root-cause analysis for a bug you have reproduced locally, post it to the ticket and file "
        "it in the Prep Cook knowledge store. Usage: /prepcook-rca <KEY>",
    ),
    Skill(
        "prepcook-fix",
        "bug:executing",
        "Commit your work with clear messages.",
        "before",
        "Implement the fix an RCA calls for, on a branch, until the reproduction passes. Usage: /prepcook-fix <KEY>",
    ),
    Skill(
        "prepcook-mr",
        "bug:executing",
        "Commit your work with clear messages.",
        "from",
        "Rebase, push and open the pull or merge request for a fix, with the reproduction as its first "
        "comment. Usage: /prepcook-mr <KEY>",
    ),
    Skill(
        "prepcook-review",
        "reviewing",
        None,
        "all",
        "Review a fix branch cold, in a fresh session, the way the orchestrator's independent reviewer does. "
        "Usage: /prepcook-review <KEY>",
    ),
]

TICKET_HEADER = "Ticket: {{ticket_key}} — {{title}}\n\nTicket description:\n{{description}}\n\n"
TICKET_LEAD = (
    "Ticket: $ARGUMENTS. Read it through your tracker's MCP before anything else — its description, its "
    "comments, its attachments and any recording linked from it (for Jira, the Atlassian MCP: `getJiraIssue`, "
    "then the comments and attachments). `<KEY>` below is that ticket's key.\n\n"
)
REVIEW_LEAD = (
    "Ticket: $ARGUMENTS. Read its title and description through your tracker's MCP (for Jira, the Atlassian "
    "MCP's `getJiraIssue`); `<KEY>` below is that ticket's key. Which of its comments you read is covered "
    "below.\n\n"
)

RULES: list[Rule] = [
    # --------------------------------------------------------- environment
    Rule(
        "environment", "bug:planning",
        "Prep Cook has already built this worktree's version of the app and started it; it listens at "
        "`$PREPCOOK_APP_URL`, seeded with the project's test data.",
        "You stood the app up yourself on this machine from the repository's `.prepcook/scripts.toml` — the "
        "manifest the orchestrator runs: `setup`, `build`, `start`, then `seed` — with `<target>` checked "
        "out; it listens at `$PREPCOOK_APP_URL`, seeded with the project's test data.",
        "On the board Prep Cook stands the app up before the agent starts; on a laptop the engineer does, from "
        "the same manifest.",
    ),
    Rule(
        "environment", "bug:planning",
        "The test login is in `$PREPCOOK_TEST_USER_EMAIL` and `$PREPCOOK_TEST_USER_PASSWORD`, plus whatever else "
        "the seed exported: read them from the environment, and never print, log, upload or commit them.",
        "The test login is in `$PREPCOOK_TEST_USER_EMAIL` and `$PREPCOOK_TEST_USER_PASSWORD`, plus whatever else "
        "the seed wrote to `$PREPCOOK_ENV_OUT` — the engineer exports those lines into the shell Claude Code "
        "runs in: read them from the environment, and never ask for them, print, log, upload or commit them.",
        "Prep Cook hands the seed's variables to the session; locally the engineer exports them, and the skill "
        "must not ask for them in chat instead.",
    ),
    Rule(
        "environment", "bug:planning",
        "`app_status` says what is running; `app_restart` rebuilds and restarts the app from the worktree's "
        "current files, which is how to test a candidate change.",
        "Rebuilding and restarting the app from the worktree's current files — the manifest's `build`, then "
        "`start` — is how to test a candidate change.",
        "`app_status` and `app_restart` are tools of the agent-mode MCP and act on apps Prep Cook started; the "
        "engineer's app is restarted with the manifest's own commands.",
    ),
    Rule(
        "environment", "bug:planning",
        "that is a seed problem, not the bug: post it on the card via `add_issue_comment`, stop, and end "
        "your turn.",
        "that is a seed problem, not the bug: say which error and where it appeared, and stop — re-running "
        "the manifest's `seed` is the engineer's call.",
        "There is no card to post on and no turn to end; the person is reading.",
    ),
    Rule(
        "environment", "bug:planning",
        "confirmed where possible by making the harness pass against a candidate change (`app_restart` after "
        "the edit), not by reading alone.",
        "confirmed where possible by making the harness pass against a candidate change (rebuild and restart "
        "the app after the edit), not by reading alone.",
        "Same as the `app_restart` rule above.",
    ),
    Rule(
        "environment", "bug:executing",
        "Run `app_restart` after your change, so the \"after\" is the fixed build. Get the \"before\" by "
        "reinstating the defect in the working tree, `app_restart`, capturing, then reverting it and "
        "restarting again, and say so when you report it.",
        "Rebuild and restart the app after your change (the manifest's `build`, then `start`), so the "
        "\"after\" is the fixed build. Get the \"before\" by reinstating the defect in the working tree, "
        "rebuilding and restarting, capturing, then reverting it and restarting again, and say so when you "
        "report it.",
        "Same as the `app_restart` rule for planning.",
    ),
    # --------------------------------------------------------------- board
    Rule(
        "board", "bug:planning",
        "name that system in `planning_integration`, whatever you conclude the cause is, including when you "
        "conclude it is our own code. The seeded app cannot hold the customer's credentials or data, so that "
        "fact changes what your reproduction is worth, and it is recorded so the person approving is told.",
        "name that system under Open questions in the RCA and in the ticket comment, whatever you conclude "
        "the cause is, including when you conclude it is our own code. The seeded app cannot hold the "
        "customer's credentials or data, so that fact changes what your reproduction is worth, and it is "
        "recorded so whoever reads the RCA is told.",
        "`planning_integration` is a card field; the RCA and the ticket are what a person reads.",
    ),
    Rule(
        "board", "bug:planning",
        "find the commit, cite it, and route the ticket as already fixed on `{{target_branch}}` and not yet "
        "released.",
        "find the commit, cite it, and say so on the ticket: fixed on `<target>`, not yet released.",
        "Routing is a board action; a comment on the ticket is the local equivalent.",
    ),
    Rule(
        "board", "bug:planning",
        " Record the artifact path on the card with the card's `update_issue` tool (spec_path = "
        "\"{{spec_path}}\").",
        "",
        "The card's spec_path field is the board's; the path is fixed by convention here.",
    ),
    Rule(
        "board", "bug:planning",
        "Post the reproduction in both places as soon as you have it: the reviewer and the reporter read "
        "different surfaces, and evidence on only one of them is invisible to the other. On the card via "
        "`add_issue_comment`, with `REPRODUCTION:` alone as the first line, then the harness's JSON lines, "
        "the mechanism in one paragraph, and \"the app at `{{target_branch}}`\" with the commit and the UTC "
        "time — that comment is the acceptance test the independent reviewer reads before it judges the "
        "branch, and a reproduction it cannot find is reported as a missing one. On the ticket via "
        "`tracker_add_comment` (when a tracker is configured), the same evidence in plain language for the "
        "reporter, who never opens the board. Screenshots are uploaded as attachments, never committed. Any "
        "question the harness cannot answer goes to the reporter in the ticket comment, or on the card when "
        "there is no ticket.",
        "Post the reproduction on the ticket as soon as you have it, through your tracker's MCP (for Jira, "
        "`addCommentToJiraIssue`): `REPRODUCTION:` alone as the first line, then the harness's JSON lines, "
        "the mechanism in one paragraph, and \"the app at `<target>`\" with the commit and the UTC time — "
        "plain enough for the reporter, and the acceptance test `/prepcook-review` reads before it judges the "
        "branch; a reproduction it cannot find is reported as a missing one. Screenshots are attached to the "
        "ticket, never committed. Any question the harness cannot answer goes to the reporter in that comment.",
        "One surface locally: the ticket. The header stays so the review skill can find it.",
    ),
    Rule(
        "board", "bug:planning",
        "When the RCA is done, record your verdict via `update_issue` planning fields: planning.kind = "
        "\"rca\", planning.open_questions, planning.confidence (low|medium|high), planning.summary.",
        "When the RCA is done, record your verdict at the top of the document: kind (rca), open questions, "
        "confidence (low|medium|high), summary.",
        "The planning fields live on the card; the document is what travels.",
    ),
    Rule(
        "board", "bug:planning",
        "Whenever open_questions is above 0, record the ask as well: planning_ask_audience — reporter or "
        "engineer — and planning_ask_request, the request itself, written in the words of whoever has to act "
        "on it.",
        "Whenever there are open questions, record the ask as well: the audience — reporter or engineer — "
        "and the request itself, written in the words of whoever has to act on it.",
        "Same rule, no field names.",
    ),
    Rule(
        "board", "bug:planning",
        "Record both or neither: with neither, the daily update says only that the ticket is waiting on an "
        "engineer, which is the honest thing to say when nobody wrote the ask down.",
        "Record both or neither.",
        "The daily update is the orchestrator's digest; there is none here.",
    ),
    Rule(
        "board", "bug:planning",
        "Also record planning.client_summary:",
        "Also write the client summary:",
        "Field name → plain words.",
    ),
    Rule(
        "board", "bug:planning",
        "It is posted on the ticket (when a tracker is configured) when the RCA is filed, so write it for "
        "someone who will never open the RCA.",
        "Post it on the ticket when the RCA is done — it is the comment an account manager forwards, so "
        "write it for someone who will never open the RCA — and file the RCA in the knowledge store with the "
        "`prepcook` MCP's `kb_file_document` (kind rca, the ticket key, work_class bug, the summary, the "
        "document's HTML as the body); the orchestrator's graph analyst links it into the graph from there.",
        "The orchestrator files and posts on approval; here the skill does both, explicitly.",
    ),
    Rule(
        "board", "bug:planning",
        "When you record confidence below high, also pass planning_confidence_reason to `update_issue` — one "
        "sentence naming the missing confirmation — and make the single most valuable unanswered question the "
        "FIRST line of the next steps in planning.summary, phrased as a direct question to the team.",
        "When you record confidence below high, add one sentence naming the missing confirmation, and make "
        "the single most valuable unanswered question the FIRST line of the next steps in the summary, "
        "phrased as a direct question to the team.",
        "Same rule, no field names.",
    ),
    Rule(
        "board", "reviewing",
        "Two comments on the card are exceptions, and you read both by calling the card's "
        "`list_issue_comments` tool before you start — they live on the board, not on the ticket, which "
        "carries only the plain-language summary. The first is the reproduction: a comment reporting the "
        "harness's cases with expected and observed values — an acceptance test, not the author's reasoning, "
        "read for the same reason you read the tests. It is headed `REPRODUCTION:` when the planning agent "
        "wrote it under the current playbook and unheaded when it did not, so the header is how you find it "
        "quickly and never what makes it count: judge a comment by whether it reports cases. The second is "
        "any comment headed `REVIEW-REJECTED: <file>:<line>`, where an earlier fix round argued that a "
        "finding does not hold. You begin each round with no memory of the last, so without these a finding "
        "that was already answered comes back in new words every round until the cap, and the ticket stalls "
        "with no verdict and nobody better informed. Where a rejection covers a finding you were about to "
        "raise, either accept it and drop the finding, or raise the finding with the rejection's argument "
        "named and answered — never as though it had never been addressed. The tool returns every comment "
        "on the card, and the rest are not yours: an earlier round's `REVIEW:` comment holds another "
        "reviewer's conclusions, and reading it would make this round an echo of that one instead of the "
        "second independent look the loop exists to get. Skim for the two headers and stop.",
        "One comment on the ticket is the exception, and you read it through your tracker's MCP before you "
        "start: the reproduction — a comment headed `REPRODUCTION:` reporting the harness's cases with "
        "expected and observed values, an acceptance test, not the author's reasoning, read for the same "
        "reason you read the tests. Judge it by whether it reports cases. Read that comment and stop; the "
        "ticket's other comments are the author's and the reporter's, and not yours.",
        "No fix rounds locally, so no rejections to honour; the reproduction is on the ticket, not a card.",
    ),
    Rule(
        "board", "reviewing",
        "If no comment on the card reports the harness's cases at all,",
        "If no comment on the ticket reports the harness's cases at all,",
        "Card → ticket.",
    ),
    Rule(
        "board", "reviewing",
        "the class and severity are what the orchestrator routes on, so weigh them as decisions, not labels.",
        "the class and severity decide what the author does next, so weigh them as decisions, not labels.",
        "Nothing routes on the verdict locally; the author does.",
    ),
    Rule(
        "board", "reviewing",
        "7. Record the outcome twice, machine-readable first. Call the card's `record_review` tool with "
        "`approved` (the verdict rule above) and every finding — class, severity, path, line, title, body. "
        "That call is what the orchestrator routes on, and it answers with how many findings will start a "
        "fix round, so check that number matches what you intended. Then post ONE comment on the card via "
        "`add_issue_comment` for people to read.",
        "7. Write ONE review to `.repro/<KEY>/review.md` and print it; the author reads it in their next "
        "session.",
        "No `record_review` and no card; the review file is what the author's next session reads.",
    ),
    Rule(
        "board", "reviewing",
        "Do not modify code, do not commit, do not push, and do not change the ticket's status — a human "
        "decides whether it advances.",
        "Do not modify code, do not commit, do not push. The author reads the review and decides: `/prepcook-fix` "
        "addresses the findings, `/prepcook-mr` opens the pull request.",
        "The human is the author, and the next skills are named.",
    ),
    Rule(
        "board", "reviewing",
        "these findings drive automated fixes, so a wrong one does real damage.",
        "these findings drive the author's next round of fixes, so a wrong one does real damage.",
        "On the board a finding starts an automated fix round; here the author acts on it.",
    ),
    Rule(
        "board", "reviewing",
        "the fix agent has no way to answer one.",
        "`/prepcook-fix` has no way to answer one.",
        "The fix agent is the board's; the fix skill is the local one.",
    ),
    # ---------------------------------------------------------------- gate
    Rule(
        "gate", "bug:planning",
        "no JS beyond what the review gate below asks for)",
        "no JS)",
        "There are no gate buttons in a local RCA.",
    ),
    Rule(
        "gate", "bug:planning",
        "Finally, open the human review gate. {{gate_instructions}} Feedback applies to the document AND to "
        "your analysis: update the verdict if it changes. Do not start the fix — a human moves the ticket to "
        "Executing when ready.",
        "Stop here and hand the RCA to its reader. Feedback applies to the document AND to your analysis: "
        "update the verdict if it changes. Do not start the fix: `/prepcook-fix` is the next step, once you or a "
        "reviewer accept the RCA.",
        "No gate, no follow-up relay, no ticket to move; the reader is at the keyboard.",
    ),
    # -------------------------------------------------------------- runner
    Rule(
        "runner", "bug:planning",
        "Run it in the foreground under `timeout` (raise the Bash tool's timeout to match) or with a single "
        "bounded blocking wait — never a loop of separate polls, and never in the background: a process still "
        "running when your turn ends dies with the turn, and its result is lost.",
        "Run it in the foreground under `timeout` (raise the Bash tool's timeout to match).",
        "Turns do not end under a person; the foreground habit is still the right one.",
    ),
    Rule(
        "runner", "bug:executing",
        "Anything long-running runs in the foreground under `timeout` (raise the Bash tool's timeout to match) "
        "or with a single bounded blocking wait — never a loop of separate polls, and never in the "
        "background: a process still running when your turn ends dies with the turn, and its result is lost.",
        "Anything long-running runs in the foreground under `timeout` (raise the Bash tool's timeout to match).",
        "Same as above.",
    ),
    Rule(
        "runner", "bug:executing",
        "Prep Cook reruns the project's scripts and tests on this branch before the {{pr_noun}} opens; you still "
        "run the tests that cover your change.",
        "Your app is the acceptance environment: rebuild and restart it from this branch and run the harness "
        "from `.repro/<KEY>/` against it until it passes, then run the manifest's `test` command once before "
        "you hand on — the check the orchestrator runs before a pull request opens.",
        "Nobody reruns the scripts for a laptop; the engineer runs the same stand-up and `test` the "
        "orchestrator's pre-merge verification does.",
    ),
    Rule(
        "runner", "bug:executing",
        "evidence goes on the {{pr_noun}} and the card, not into the branch, and a diff that touches repro/ or "
        ".repro/ is refused at the push gate.",
        "evidence goes on the pull request, not into the branch.",
        "No card and no push gate; the rule itself stands.",
    ),
    # ------------------------------------------------------------ template
    # Last among the playbook-wide rules, so every rule above quotes the
    # playbook as written.
    Rule(
        "template", "bug:planning", TICKET_HEADER, TICKET_LEAD,
        "The orchestrator renders the ticket into the prompt; a skill reads it through the tracker's MCP.",
    ),
    Rule(
        "template", "reviewing", TICKET_HEADER, REVIEW_LEAD,
        "Same, but the reviewer reads only the description here; its one comment is named further down.",
    ),
    Rule(
        "template", "bug:executing", "Ticket: {{ticket_key}} — {{title}}\n\n", TICKET_LEAD,
        "The executing playbook carries no description; the skill still needs the key.",
    ),
    Rule(
        "template", "bug:planning", "{{spec_path}}", "docs/specs/<KEY>.html",
        "The default spec path, spelled out.", many=True,
    ),
    Rule(
        "template", "bug:executing", "{{spec_path}}", "docs/specs/<KEY>.html",
        "Same as above.", many=True,
    ),
    Rule(
        "template", "bug:planning", "{{ticket_key}}", "<KEY>",
        "A placeholder the reader fills from $ARGUMENTS.", many=True,
    ),
    Rule(
        "template", "bug:executing", "{{ticket_key}}", "<KEY>",
        "Same as above.", many=True,
    ),
    Rule(
        "template", "bug:planning", "{{target_branch}}", "<target>",
        "The orchestrator reads the target from the repository; each preamble says how to find it.", many=True,
    ),
    Rule(
        "template", "bug:executing", "{{target_branch}}", "<target>",
        "Same as above.", many=True,
    ),
    Rule(
        "template", "reviewing", "{{target_branch}}", "<target>",
        "Same as above.", many=True,
    ),
    Rule(
        "template", "bug:planning", "{{pr_noun}}", "pull request",
        "The host decides the noun; the preambles say a GitLab merge request is meant too.", many=True,
    ),
    Rule(
        "template", "bug:executing", "{{pr_noun}}", "pull request",
        "Same as above.", many=True,
    ),
    Rule(
        "template", "reviewing", "{{pr_noun}}", "pull request",
        "Same as above.", many=True,
    ),
    # ------------------------------------------------- per-skill, after the cut
    Rule(
        "gate", "prepcook-repro",
        "Investigate this bug and produce a root-cause analysis — do NOT implement the fix in this session.\n\n",
        "Reproduce this bug and keep the script that does it — do NOT diagnose or fix it in this session; "
        "`/prepcook-rca` reads what you leave under `.repro/<KEY>/`.\n\n",
        "The repro half stops at the reproduction; the playbook's lead promised the whole RCA.",
    ),
    Rule(
        "gate", "prepcook-repro",
        "Record the UTC time of the reproduction in the RCA, and the commit the app was built from "
        "(`git rev-parse HEAD`).",
        "Record the UTC time of the reproduction and the commit the app was built from (`git rev-parse HEAD`) "
        "in `.repro/<KEY>/NOTES.md`; `/prepcook-rca` carries them into the RCA.",
        "There is no RCA yet when the repro skill runs; the notes file is what it hands on.",
    ),
    Rule(
        "gate", "prepcook-rca",
        "With the failure in hand, find the mechanism:",
        "With the failure in hand — the harness under `.repro/<KEY>/` fails on `<target>`; if it does not, "
        "run `/prepcook-repro` first — find the mechanism:",
        "The RCA half starts where the repro half ended, and says so.",
    ),
    Rule(
        "gate", "prepcook-mr",
        " Do not push or open a pull request — a human moves the ticket to Reviewing when ready.",
        "",
        "This skill is the push; the fix skill's tail sends the reader through review first.",
    ),
    Rule(
        "gate", "prepcook-mr",
        "Commit your work with clear messages.",
        "Commit your work with clear messages, if `/prepcook-fix` left anything uncommitted.",
        "The MR half starts where the fix half ended.",
    ),
]

# What each skill's generated body is followed by. Hand-written, kept here so
# the generator has one source and a tweak to it is a diff like any other.
TAILS = {
    "prepcook-fix": (
        "Stop here once the harness passes and the tests are green: commit, run `/prepcook-self-review` here, "
        "then `/prepcook-review` in a fresh session, and `/prepcook-mr` after it."
    ),
    "prepcook-mr": (
        "Then push the branch and open the pull request against `<target>` with `gh` or `glab`, whichever "
        "your host uses (on GitLab it is a merge request): the title from the ticket; the description in the "
        "shape Summary, Changes, Verification, and a line that closes `<KEY>` the way your tracker links "
        "them; and the reproduction as its first comment — a four-backtick block with the harness's source "
        "from the RCA's Reproduction section, read through once more for anything that looks like a "
        "credential, and the pasteable kickoff prompt from its Reproduce locally section under "
        "`<details open>`. Link the pull request on the ticket through your tracker's MCP (for Jira, a "
        "remote link); the ticket's status is your team's workflow, not this skill's. A human reviewer is "
        "assigned by the project's rules; do not approve your own."
    ),
}

# The two supporting skills are copied from assets/agent-skills; these rules
# target a file by its skill-relative path. A file listed with no rules is
# copied as it is, under the generated note.
COPIED_FILES = {
    "repro-harness": ["SKILL.md", "harness.template.mjs"],
    "evergreen-comments": ["SKILL.md"],
}

SHIPPED_NOTE = (
    "<!-- Shipped by Prep Cook from assets/agent-skills/. Edits made\n"
    "     here are overwritten on the next start; change the repo copy instead. -->\n\n"
)

COPIED_RULES: list[Rule] = [
    Rule(
        "runner", "repro-harness/SKILL.md", SHIPPED_NOTE, "",
        "Prep Cook overwrites its installed copy on start; this copy says how it is made in the generated note.",
    ),
    Rule(
        "runner", "evergreen-comments/SKILL.md", SHIPPED_NOTE, "",
        "Same as above.",
    ),
    Rule(
        "environment", "repro-harness/SKILL.md",
        "against the app Prep Cook stood up for this worktree (at $PREPCOOK_APP_URL) and later proves a fix.",
        "against the app you stood up from .prepcook/scripts.toml (at $PREPCOOK_APP_URL) and later proves a fix.",
        "The skill's one-line description names who started the app.",
    ),
    Rule(
        "board", "repro-harness/SKILL.md",
        "Evidence is uploaded to the card and the merge\n"
        "request as attachments. The push gate refuses a diff that touches `repro/` or\n"
        "`.repro/`.",
        "Evidence is attached to the ticket and the pull\n"
        "request, never committed.",
        "No card, no push gate.",
    ),
    Rule(
        "environment", "repro-harness/SKILL.md",
        "Prep Cook has already built this worktree's version of the app and started it, from\n"
        "the repository's `.prepcook/scripts.toml`. It listens at `$PREPCOOK_APP_URL`, seeded with\n"
        "the project's test data. `app_status` says what is running; `app_restart` rebuilds\n"
        "and restarts it from the worktree's current files, which is how a harness is run\n"
        "against a candidate change.",
        "You stood this worktree's version of the app up yourself, from the repository's\n"
        "`.prepcook/scripts.toml` — `setup`, `build`, `start`, `seed`, the commands the\n"
        "orchestrator runs. It listens at `$PREPCOOK_APP_URL`, seeded with the project's test\n"
        "data. Rebuilding and restarting it from the worktree's current files (the\n"
        "manifest's `build`, then `start`) is how a harness is run\nagainst a candidate change.",
        "The engineer, not Prep Cook, stood the app up; `app_status` and `app_restart` act only on apps Prep Cook started.",
    ),
    Rule(
        "environment", "repro-harness/SKILL.md",
        "whatever else the seed exported (`app_status` lists the names).",
        "whatever else the seed wrote to `$PREPCOOK_ENV_OUT` and you exported.",
        "The seed's variables reach a laptop session because the engineer exports them.",
    ),
    Rule(
        "board", "repro-harness/SKILL.md",
        "that is a seed problem, not something to work\naround: post it on the card and stop.",
        "that is a seed problem, not something to work\naround: say which and stop.",
        "No card to post on.",
    ),
    Rule(
        "board", "repro-harness/SKILL.md",
        "The orchestrator and the reviewer read the\n  exit code",
        "`/prepcook-fix` and `/prepcook-review` read the\n  exit code",
        "The readers are the next skills.",
    ),
    Rule(
        "runner", "repro-harness/SKILL.md",
        "Raise the Bash tool's own timeout to match. Never start the harness in the\n"
        "background and end the turn — a process still running when the turn ends dies\n"
        "with it and the result is lost. If you must wait on something external, one bounded\n"
        "blocking wait, not a loop of separate polls.",
        "Raise the Bash tool's own timeout to match, and keep the harness in the foreground so\n"
        "its exit code is what you report.",
        "Turns do not end under a person.",
    ),
    Rule(
        "board", "repro-harness/SKILL.md",
        "On the card (`add_issue_comment`), with `REPRODUCTION:` alone as the first line:\n"
        "the JSON lines verbatim, the mechanism in one paragraph, \"the app at\" the target\n"
        "branch (or the fix branch) with the commit and the UTC time; screenshots attached. "
        "That header is what the independent\n"
        "reviewer looks for, and a reproduction it cannot find is reported as a missing one.\n"
        "On the ticket (`tracker_add_comment`): the same evidence in plain language, for\n"
        "the reporter, who never opens the board. In the RCA:",
        "On the ticket, through your tracker's MCP, with `REPRODUCTION:` alone as the first\n"
        "line: the JSON lines verbatim, the mechanism in one paragraph, \"the app at\" the\n"
        "target branch (or the fix branch) with the commit and the UTC time; screenshots\n"
        "attached. That header is what `/prepcook-review` looks for, and a reproduction it cannot\n"
        "find is reported as a missing one. Write it plainly enough for the reporter, who\n"
        "reads the same comment. In the RCA:",
        "One surface locally: the ticket.",
    ),
    Rule(
        "board", "repro-harness/SKILL.md",
        "error on the app is a finding to post on the card, not a flag to set.",
        "error on the app is a finding to report, not a flag to set.",
        "No card.",
    ),
    Rule(
        "environment", "repro-harness/harness.template.mjs",
        "// The app's address and the test login come from the environment Prep Cook sets when it\n"
        "// stands the app up (PREPCOOK_APP_URL, PREPCOOK_TEST_USER_EMAIL, PREPCOOK_TEST_USER_PASSWORD) and\n"
        "// are never written anywhere.",
        "// The app's address and the test login come from the environment you exported when you\n"
        "// stood the app up from .prepcook/scripts.toml (PREPCOOK_APP_URL, PREPCOOK_TEST_USER_EMAIL,\n"
        "// PREPCOOK_TEST_USER_PASSWORD) and are never written anywhere.",
        "The engineer exports the seed's variables; Prep Cook does not.",
    ),
    Rule(
        "runner", "repro-harness/harness.template.mjs",
        "    // Playwright's bundled Chromium unless the machine names its own.\n"
        "    executablePath: process.env.PREPCOOK_CHROMIUM_PATH || undefined,\n"
        "    args: ['--no-sandbox'],",
        "    // Playwright's bundled Chromium unless the machine names its own; HEADED=1 to watch a run.\n"
        "    executablePath: process.env.PREPCOOK_CHROMIUM_PATH || undefined,\n"
        "    headless: !process.env.HEADED,",
        "The orchestrator's sandbox needs --no-sandbox; a laptop does not, and can show the browser.",
    ),
    Rule(
        "board", "repro-harness/harness.template.mjs",
        "// to post on the card, not a flag to set here.",
        "// about your setup, not a flag to set here.",
        "No card.",
    ),
]
