# Screenshots

The images the README and the docs show, captured from a running Prepcook by
[`scripts/screenshots.mjs`](../../scripts/screenshots.mjs). Each is taken at
1440×900 with device scale 2, so every file is **2880×1800 PNG**, once in the
light theme (`<name>.png`) and once in the dark (`<name>-dark.png`); the
README shows the dark one to readers whose system is in dark mode.

| File | What it must show | Used in |
| --- | --- | --- |
| `board.png` | The Pantry board's five columns with six cards: one in To Do; two in Plan, a bug waiting at the gate (amber header, **Approve** and **Review**) and a triaged investigation (amber **Needs you**, "Triaged — report ready", **Convert to bug** and **Close**); one each in Execute and Review; and one in Shipped with its pull request merged (the **Merged** chip and the amber **Merged — close card** with **Clean up**). The Clean up rail folded on the right, and the header's "3 need you" pill. | [README](../../README.md), under the opening paragraph |
| `card-rca.png` | The Plan card ("Sharing a list fails for a user who exists") open, its `REPRODUCTION:` comment and the RCA comment in view. | [README](../../README.md#3-plan-reproduce-and-explain-about-5-minutes-then-10-to-20-waiting), Quickstart step 3 |
| `card-triaged.png` | The triaged investigation in Plan open, its amber **Needs you** header ("Triaged — report ready", **Close**) and its `TRIAGE REPORT` in view. | [method.md](../method.md#investigations), Investigations |
| `workspace-review.png` | A workspace with its Review panel open. Needs a workspace, which only a card worked by the orchestrator has. | Not yet — the script skips it until a card has been worked ([review.md](../review.md)) |
| `settings-integrations.png` | Settings → Integrations → Notifications, with one webhook target. | [README](../../README.md#integrations), Integrations |
| `settings-ticketing.png` | Settings → Integrations → Ticketing. | [incidents.md](../incidents.md#setting-up-pagerduty), Setting up PagerDuty |
| `settings-evidence.png` | Settings → Integrations → Evidence. | [evidence.md](../evidence.md#configuring-sources), Configuring sources |
| `settings-agents.png` | Settings → Integrations → Agents. | [method.md](../method.md#which-agent-runs-which-stage), Which agent runs which stage |
| `settings-repositories.png` | Settings → Repositories with `prepcook-demo` selected and its Scripts card. | [scripts.md](../scripts.md#trying-it-from-settings), Trying it from Settings |
| `quickstart-stand-up.png` | The Scripts card after **Test stand-up**: outcome `ok`, the `setup`, `start`, `ready` and `seed` steps at exit code 0. Run Test stand-up before capturing; otherwise the shot has the card without a run. | [README](../../README.md#2-add-the-demo-repository-about-5-minutes), Quickstart step 2 |
| `knowledge.png` | The board's Knowledge page, the RCA graph. Shows RCAs only once a real planning verdict has been filed. | Not yet — captured, but empty until a real planning verdict is filed ([rca-graph.md](../rca-graph.md)) |

Each also has its `-dark` twin.

## Regenerating

With Prepcook running (`prepcook up`), the orchestrator switched off
(**Settings → Orchestrator → Enable automatic agent launches**, so the seeded
cards start no agents), a clone of `prepcook-demo` beside this repository, and
`PREPCOOK_API_TOKEN` exported:

```sh
pnpm screenshots -- --seed
```

`--seed` puts the sample board on first and is safe to repeat. Without it the
script captures whatever the board holds. `--only board,card-rca` limits the
shots, `--list` prints them, and the script's header lists every option, what
it can and cannot seed, and what it masks (the home directory and the owner's
email are covered before each capture). The first run needs Playwright's
browser: `npx playwright install chromium`.
