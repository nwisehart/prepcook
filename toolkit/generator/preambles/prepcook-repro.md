Before you start, three checks, each a sentence in your first reply:

- The app: it is up on this machine, stood up from the repository's `.prepcook/scripts.toml` the way the orchestrator stands it up — `setup`, `build`, `start` (listening on `$PREPCOOK_PORT`), then `seed` — with `<target>` checked out, and `$PREPCOOK_APP_URL` answers. If not, say what is missing and stop: standing the app up is the engineer's step, not one to improvise.
- The login: `PREPCOOK_APP_URL`, `PREPCOOK_TEST_USER_EMAIL` and `PREPCOOK_TEST_USER_PASSWORD` are set in the environment Claude Code was started from — the last two are lines the seed wrote to `$PREPCOOK_ENV_OUT`. Check that they are set, never what they are; never ask for them in chat and never echo them.
- The ticket: your tracker's MCP is connected and the ticket opens through it.

The harness goes under `.repro/<KEY>/`, which is never committed.
