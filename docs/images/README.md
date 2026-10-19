# Screenshots

The images the README and the docs show, captured from a running Prepcook by
[`scripts/screenshots.mjs`](../../scripts/screenshots.mjs). Each is taken at
1440×900 with device scale 2, so every file is **2880×1800 PNG**, once in the
light theme (`<name>.png`) and once in the dark (`<name>-dark.png`); the
README shows the dark one to readers whose system is in dark mode.

| File | What it must show | Used in |
| --- | --- | --- |
| `board.png` | The Pantry board with six cards, one each in To Do, Plan, Triaged, Execute, Review and Close. | [README](../../README.md), under the opening paragraph |
| `card-rca.png` | The Plan card ("Sharing a list fails for a user who exists") open, its `REPRODUCTION:` comment and the RCA comment in view. | [README](../../README.md#3-plan-reproduce-and-explain-about-5-minutes-then-10-to-20-waiting), Quickstart step 3 |
| `card-triaged.png` | The Triaged investigation open, its `TRIAGE REPORT` in view. | Not yet ([method.md](../method.md#investigations)) |
| `workspace-review.png` | A workspace with its Review panel open. Needs a workspace, which only a card worked by the orchestrator has. | Not yet ([review.md](../review.md)) |
| `settings-integrations.png` | Settings → Integrations → Notifications, with one webhook target. | [README](../../README.md#integrations), Integrations |
| `settings-ticketing.png` | Settings → Integrations → Ticketing. | Not yet ([incidents.md](../incidents.md)) |
| `settings-evidence.png` | Settings → Integrations → Evidence. | Not yet ([evidence.md](../evidence.md)) |
| `settings-agents.png` | Settings → Integrations → Agents. | Not yet ([configuration.md](../configuration.md#agents)) |
| `settings-repositories.png` | Settings → Repositories with `prepcook-demo` selected and its Scripts card. | Not yet ([scripts.md](../scripts.md)) |
| `quickstart-stand-up.png` | The Scripts card after **Test stand-up**: outcome `ok`, the `setup`, `start`, `ready` and `seed` steps at exit code 0. Run Test stand-up before capturing; otherwise the shot has the card without a run. | [README](../../README.md#2-add-the-demo-repository-about-5-minutes), Quickstart step 2 |
| `knowledge.png` | The board's Knowledge page, the RCA graph. Shows RCAs only once a real planning verdict has been filed. | Not yet ([rca-graph.md](../rca-graph.md)) |

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
