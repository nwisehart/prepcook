# Contributing to Prepcook

Thanks for helping. Prepcook is small enough that one person reads every pull
request, so keep changes focused and say what they are for.

## Before you start

- Open an issue for anything bigger than a bug fix, so the shape can be
  agreed before the code is written.
- Read [docs/developing.md](docs/developing.md) for the architecture, the
  repository layout and the build and test commands, and
  [AGENTS.md](AGENTS.md) for the coding conventions (the same file is what
  coding agents read).
- Write the name as **Prepcook** in prose and the UI, and `prepcook` in code,
  binaries (`prepcook`, `prepcook-mcp`), packages (`@prepcook/*`), paths
  (`.prepcook/scripts.toml`) and skills (`prepcook-*`); environment variables
  take the `PREPCOOK_` prefix.

## Pull requests

1. Branch from `main`.
2. Keep the change to one topic. A refactor and a feature are two pull
   requests.
3. Run `pnpm run format`, `pnpm run lint` and `pnpm run check` before you
   push; CI runs the same.
4. Add or update tests for behaviour you change. Rust tests live beside the
   code; frontend logic gets a Vitest file in the same directory.
5. Write comments that read cold: document the current behaviour and the
   constraint behind it, never the change that produced it or the ticket it
   came from.
6. Describe in the pull request what changed, why, and how you verified it.

Before a release, run [docs/manual-testing.md](docs/manual-testing.md): a
cold start and every integration, by hand, on a real machine.

## Adding a provider or a coding agent

Every integration area (ticketing, incidents, notifications, evidence,
reproduction) and the coding agents are extended the same way: a file, a few
enum arms and a descriptor, with Settings and the configuration reference
generated from it. [docs/developing.md](docs/developing.md#the-factory-pattern)
walks through each.

## Playbooks and skills

The stage playbooks and the toolkit skills generated from them are part of the
product: a change to a playbook's text needs the same care as a code change,
so say which stage it affects and what you observed before and after.
[docs/developing.md](docs/developing.md#playbooks-and-the-toolkit) covers
where they live and how to regenerate the skills.

## License

By contributing you agree that your contributions are licensed under the
Apache License, Version 2.0, the license of this project.
