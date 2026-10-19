# Board service

The `remote` crate is the board service: issues, comments, tags, pull-request
links, the knowledge store and the root-cause graph, kept in Postgres and
streamed to the UI through ElectricSQL. It runs in Docker; the native Prepcook
server serves the UI and talks to it.

## Run

`prepcook up` runs this stack from the published image
`ghcr.io/nwisehart/prepcook-board:<version>`, the version of the `prepcook` binary
that starts it. The binary embeds
[`docker-compose.release.yml`](docker-compose.release.yml) and writes it,
with a generated `.env` (database passwords and the JWT secret), to
`<app data dir>/board/`: `~/.local/share/prepcook/board` on Linux,
`~/Library/Application Support/ai.bloop.prepcook/board` on macOS
(`dev_assets/board` for a debug build). `prepcook down` stops it and keeps the data; `prepcook down
--volumes` deletes it too.

The stack is `remote-db` (Postgres 16), `electric` and `remote-server`
(port 4110 on localhost). The first start creates the default organization.

## Authentication

Nobody signs in to the board. The native server holds the board's
`PREPCOOK_JWT_SECRET` as its `PREPCOOK_BOARD_SECRET` (`prepcook up` reads it
from the board `.env`) and signs a short-lived owner token (HS256,
`aud = "prepcook-owner"`, `iat`, `exp` at most an hour later) for every
request. The board accepts it as its owner: the user `owner@localhost`, created
on first use as an admin of the default organization, in one persistent
session. Agent users, which admins create per organization, sign in with their
email and secret at `POST /v1/auth/agent/login` and get session tokens that
refresh at `POST /v1/tokens/refresh`.

## From source

[`docker-compose.yml`](docker-compose.yml) builds the image from this
checkout instead. Either let `prepcook up` start it:

```bash
prepcook up --compose crates/remote/docker-compose.yml   # same board .env as above
```

With `--compose`, `prepcook up` passes `--build`, so the image is rebuilt from
the checkout on every start rather than reused from an earlier one.

or run it on its own with `crates/remote/.env.remote` (gitignored):

```env
# Required
DB_PASSWORD=replace_with_a_strong_password
ELECTRIC_ROLE_PASSWORD=replace_with_a_strong_password
PREPCOOK_JWT_SECRET=replace_with_openssl_rand_base64_48

# Optional
PUBLIC_BASE_URL=http://localhost:4110
```

Generate the JWT secret once with `openssl rand -base64 48`. Then
`pnpm run remote:dev` builds the image from source and runs the stack in the
foreground (it also publishes Postgres on port 4115 for
`remote:prepare-db`), and tears it down on exit. Start the server against it
with the same secret:

```bash
PREPCOOK_SHARED_API_BASE=http://127.0.0.1:4110 \
PREPCOOK_BOARD_SECRET="$(sed -n 's/^PREPCOOK_JWT_SECRET=//p' crates/remote/.env.remote)" \
  cargo run --bin prepcook
```

## Development

```bash
pnpm run remote:dev           # build and run the stack, tear down on exit
pnpm run remote:dev:clean     # remove containers and the database volume
pnpm run remote:generate-types
pnpm run remote:prepare-db    # refresh the offline SQLx query data (needs Postgres)
```

See [AGENTS.md](AGENTS.md) for the architecture, the ElectricSQL sync model
and the mutation pattern.
