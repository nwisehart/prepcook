# The knowledge store and the root-cause graph

One RCA explains one bug. Fifty RCAs say where bugs come from. Prepcook keeps every
bug RCA in a knowledge store and links each one into a graph of product areas,
components and root-cause classes, so a cause that keeps coming back becomes
visible and can be reviewed as a pattern rather than fixed once more as a
symptom.

The store lives on the board, so every teammate's Prepcook reads and writes the
same one. The board's **Knowledge** menu shows it: a ranking of root-cause
classes, the graph as a web, and an export.

## What an RCA document is

The RCA the planning agent wrote, as filed: a document of kind `rca` holding
the HTML itself (so it outlives the worktree), the card's title, the card and
its tracker key when it has one, and the verdict's summary. Bug cards and
investigation cards ([method.md](method.md#investigations)) are filed; an
investigation's document carries `work_class: investigation`, and it is filed
only once, when its verdict is recorded, since an investigation is never
approved.

It is filed twice, into the same document: once when the planning verdict is
recorded, so teammates can read the draft while it waits at the gate, and again
on approval. When a tracker is configured and the card has a ticket, the
document is pinned to the ticket as a link, and on approval the plain-language
summary is posted there.

An RCA written at someone's terminal with the [toolkit](../toolkit/README.md)
is filed into the same store by `/prepcook-rca`, through the `prepcook` MCP's
`kb_file_document`, keyed by its ticket.

## Tags

Tags are `name:value` strings on a document. Four are set mechanically when it
is filed: `board:<label>` (the board's label), `severity:p0`…`p3` (the card's
priority), `confidence:<level>`, and `customer:<slug>` when the title names one
in parentheses.

The rest are judgments, added by the analysis agent:

| Tag | Values | Meaning |
| --- | --- | --- |
| `origin:` | `regression`, `dormant`, `unclear` | Exactly one. A recent change introduced it, the flaw was long-standing, or the evidence does not say. |
| `culprit:` | `orchestrator`, `human` | With `origin:regression` and an identified commit: whether that commit's author is the orchestrator's own identity or a person. |
| `detectable-by:` | `code-review`, `qa`, `automated-tests` | When the evidence clearly shows where it could have been caught. A document can carry several. |
| `area:` | the product area's slug | Exactly one, matching the product-area node it is linked to, e.g. `area:sharing`. |
| `fix-type:` | `code`, `config`, `prompt`, `data` | Exactly one: what kind of change fixed it. |
| `surface:` | `client`, `backend`, `worker`, `agent`, `integration` | Exactly one: the deploy unit at fault. |
| `repeat-of:` | a ticket key | When the RCA cites a prior ticket with the same root cause. |
| `dormant-age:` | `<1m`, `<6m`, `<1y`, `1y+` | With `origin:dormant` and an identifiable commit: how old that commit was when the bug was reported. |

These are counted to improve the process; they are not graph links.

## The graph

Nodes are of three kinds:

- **product area**: a part of the product people would name (Sign-in,
  Sharing);
- **component**: a part of the code (the session cookie, the JSON store);
- **root-cause class**: a kind of mistake ("stale cache invalidation",
  "case-sensitive comparison of user input").

Product areas and components are seeded and kept by the team. Each RCA is
linked to one product area, one component when one is clearly identifiable,
and one or two root-cause classes. Every link carries a one-line rationale and a
confidence.

The linking is done by the `rca_analysis` playbook, run from the board menu
(**Analyze RCAs**) or by the orchestrator whenever approved RCAs are waiting to
be linked. It reads only the documents not yet linked, never re-reads linked
ones, and never runs two at once for a board.

**Attach first, invent last.** The agent creates a node only when no existing
one plausibly fits, and only a root-cause class. New classes are named
generically enough to attract future links ("stale cache invalidation", not
"dashboard cache bug"). When torn between an existing node and a new one, it
picks the existing node with lower confidence. A graph of a hundred one-RCA
classes says nothing; node sprawl is the failure this rule exists to prevent.

## Pattern reviews

When a node collects as many linked RCAs as the **pattern-review threshold**
(`orchestrator.pattern_threshold`, default 5; **Settings → Orchestrator**,
overridable per board), the orchestrator files a pattern review:

- With a tracker, a P2 story in the board's tracker project, titled
  `[Pattern] <node>: recurring <kind> across <n> bugs`, plus a card in
  To Do. Without one, the card alone.
- The card is a story with the node recorded on it, assigned to you, High on
  the board. Its description lists the linked RCAs, one line each (up to
  twenty).
- The node is stamped, so it fires once. It fires again after the same number
  of new RCAs land on it.

The card is marked with the node as `pattern_node` in its metadata (cards
filed before the rename carry it as `investigation_node`, which is still
read). Moved to Plan, it takes the `pattern:planning` playbook instead of the
story one. The agent reads every linked RCA, finds where they cluster, works
out what structural thing makes the class possible, and recommends what would
stop it, each recommendation with how many of the linked RCAs it would have
caught (`planning.kind = "pattern"`). It writes no code and no spec. The report
goes through the same review gate as any plan, a person decides what, if
anything, becomes work, and the card can never pass the Executing gate.

A pattern review is not an investigation card: that is a work class of its
own, for one reported problem to reproduce and explain
([method.md](method.md#investigations)).

## `/api/stats`

Read-only numbers from the same records, for anyone who wants them without
opening the board:

```
GET /api/stats                   everything below, plus the running version
GET /api/stats/merge-requests
GET /api/stats/rcas
```

Each takes `?since=YYYY-MM-DD` (UTC, inclusive) and `?project_id=<board id>`;
without a board, every board under **Settings → Orchestrator** is counted.

- **Merge requests**: opened, merged, closed unmerged, still open, and the merge
  rate, in total and per repository. The list behind them is cached for five
  minutes.
- **RCAs**: the total; counts by `origin:`, by `culprit:` among regressions, by
  `severity:` and by `detectable-by:`; the root-cause classes with how many RCAs
  each has in range, most first, and the pattern review each one opened; and
  the number of pattern reviews filed. (The JSON keeps the field names
  `investigation_jira_key` and `investigations_filed`, which are columns on the
  board's graph.)
