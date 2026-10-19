TRIAGE REPORT

## What happens

Ben ticks items off as bought on his phone in the shop. Ann, who has the same list open at home, still sees them as still to buy, and the count in the list header does not go down.

## Where

Any shared list, on the list page, for the person who did not tick the item; the person who ticked it sees the change at once. The Pantry app log shows each tick saved (the requests answer with a redirect back to the list) and no error after any of them.

## Why

Pantry draws a list once, when the page is opened, and does not update a page that is already open when someone else changes the list. Ben's ticks are saved straight away; Ann's page is showing the list as it was when she opened it.

## Scope

Everyone who shares a list, on every list. This is how Pantry works today rather than a defect: the harness shows Ann's page catching up as soon as she reloads it, and the help pages describe the same behaviour under "Why don't I see changes someone else just made?".

## Workaround

Reload the page to see what others have added or ticked off since it was opened.

## Confidence

High — reproduced with two people signed in at once, and a reload shows every change.
