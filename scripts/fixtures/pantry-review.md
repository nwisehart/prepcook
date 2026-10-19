REVIEW: APPROVE

The reproduction's cases (a blank name, and a name of spaces only) now keep the old name and say "A list needs a name."; the harness passes against the branch, and `npm test` passes with the two new cases in `test/lists.test.js`.

1. [systematic/nit] src/routes/lists.js:67 — the blank-name check trims again after `cleanName`

   `cleanName` already trims and clips the name it returns, so `name.trim()` in the new check repeats it. Test `name` directly, as the create-list handler beside it does.

Done well: the error is rendered through `listView` with the typed value kept in the field, the same way the share form reports its errors.
