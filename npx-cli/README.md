# prepcook

A prep cook does the prep so the chef can cook. Prepcook does the prep on a
bug — reproduces it, explains it, fixes it, reviews the fix, opens the pull
request — and the engineer who owns the ticket signs off. Each ticket is a card
on a local board, each stage of the card is worked by a coding agent in its own
git worktree, and when a person takes the ticket over, Prepcook steps aside.

## Usage

```sh
npx prepcook up        # start the board and the server, and open the UI
npx prepcook --help    # list the commands
npx prepcook mcp       # run the MCP server (prepcook-mcp)
npx prepcook --version # print this package's version
```

Every other argument goes to the `prepcook` binary unchanged, so
`npx prepcook status` and `npx prepcook down` work as they do for an installed
binary.

## What the package does

On first run it downloads the release tarball for your platform from the
GitHub release matching the package version, checks it against the release's
`SHA256SUMS`, unpacks it into `~/.prepcook/bin` and runs the binary from there;
later runs use the cached copy. Because Node downloads the tarball, macOS does
not quarantine the binaries and Gatekeeper does not block the first run.

It needs macOS (Apple silicon or Intel) or Linux x64, and Node 20 or later.
The rest of the quickstart needs Docker with Compose, git, Claude Code signed
in, and the GitHub CLI for pull requests; see the
[Quickstart](https://github.com/nwisehart/prepcook#quickstart).

## Links

- Repository: https://github.com/nwisehart/prepcook
- Documentation: https://nwisehart.github.io/prepcook/
- Releases: https://github.com/nwisehart/prepcook/releases

## License

Apache-2.0. Prepcook is a fork of
[vibe-kanban](https://github.com/BloopAI/vibe-kanban) by BloopAI; see
[NOTICE](https://github.com/nwisehart/prepcook/blob/main/NOTICE).
