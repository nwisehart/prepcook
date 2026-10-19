RCA: sharing looks the email up case-sensitively

**Confidence: high.** Reproduced on `main` ({{commit}}): the harness fails for the reported reason with `Ann@Example.com` and passes with the address in lower case. The full analysis is `docs/specs/{{key}}.html` in the worktree.

## What happens

Ben shares the Weekend list with `Ann@Example.com`. The page answers "No user with that email" and nothing is shared, although Ann has an account and signs in every day. Typed in lower case, the same address works.

## Why

Pantry keeps every account's email in lower case: the seed writes them that way, and sign-in (`POST /login` in `src/routes/session.js`) lower-cases what it is given before it looks the account up, which is why Ann can sign in however she types her address. The share handler (`POST /lists/:listId/share` in `src/routes/sharing.js`) trims the typed address but does not lower-case it, then looks for an exact match in `data.users`. One capital letter and nothing matches, so the handler answers 404 with "No user with that email".

## Where else

Nothing else in Pantry looks an account up by email. Sign-in already normalises; the list and item routes work by user id.

## The fix

Normalise the address in the share handler exactly as sign-in does (`trim().toLowerCase()`), through one small helper both routes call so the two cannot drift apart again. Add a case to `test/sharing.test.js` that shares with a mixed-case address and checks the list appears under "Shared with me".

## Open questions

None. The data file holds no mixed-case emails, so nothing needs migrating.
