# Security

Prep Cook runs coding agents against your own repositories with the credentials
you give it. Treat it like any other tool that holds tokens.

## Reporting a vulnerability

Please do not open a public issue for a security problem. Use GitHub's
private vulnerability reporting on this repository (Security → Report a
vulnerability). You will get an acknowledgement within a few days and a fix
or a mitigation as soon as one exists.

## What Prep Cook does with secrets

- Tracker, git-host and notification tokens and per-repository secrets live
  in a secrets file outside the repository, or in environment variables.
  They are never written to a repository, a prompt, a log, an RCA or a
  comment; the orchestrator scrubs every secret value from agent output.
- Agents run on the machine that runs Prep Cook, with that machine's access.
  Give the orchestrator a bot identity on your tracker and git host with the
  narrowest scopes that work.
- Reproduction harnesses receive test-user credentials only through
  environment variables, and the harness source is checked for anything
  that looks like a credential before it is published in an RCA.

## Supported versions

Only the latest release receives security fixes.
