# Contributing to Prepcook

Thanks for helping. Prepcook is small enough that one person reads every pull
request, so keep changes focused and say what they are for.

## Before you start

- Open an issue for anything bigger than a bug fix, so the shape can be
  agreed before the code is written.
- Read `AGENTS.md` for the repository layout, build commands and coding
  conventions. The same file is what coding agents read.
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

## Adding a coding agent

An agent is an executor, and adding one is an enum edit in three places:

1. **The executor module**, `crates/executors/src/executors/<agent>.rs`: a
   struct implementing `StandardCodingAgentExecutor` (spawn, follow-up, log
   normalisation, `default_mcp_config_path`, availability) and its
   `descriptor()` — display name, summary, the command a session starts
   from, how it signs in, how its sessions get MCP servers, and its economy
   variant if it has a cheaper model.
2. **The `CodingAgent` enum**, `crates/executors/src/executors/mod.rs`: a
   variant for it. The compiler then points at the matches that must name it
   (`capabilities`, and `BaseCodingAgent::descriptor` in
   `crates/executors/src/descriptor.rs`); `get_mcp_config` needs an arm only
   when its MCP servers live under a key other than `mcpServers`.
3. **`crates/executors/default_profiles.json`**: its `DEFAULT` variant, plus
   any variant the descriptor names as its economy variant.

Then run `pnpm run generate-types` and `scripts/gen-config-docs.py`: Settings →
Agents and the coding-agents table in `docs/configuration.md` come from the
descriptor, so nothing else needs editing.

## Playbooks and skills

The stage playbooks (`crates/services/src/services/config/versions/`) and the
local toolkit skills generated from them are part of the product. A change to
a playbook's text needs the same care as a code change: say which stage it
affects and what you observed before and after.

## License

By contributing you agree that your contributions are licensed under the
Apache License, Version 2.0, the license of this project.
