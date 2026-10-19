# Incidents

Prepcook can be a first responder's second pair of hands. In **incidents
mode** its source is an incident tool rather than a ticket tracker: each open
incident becomes an [investigation](method.md#investigations) card, Prepcook
reproduces the problem and explains it, and the triage report goes back to the
incident as a note for the people working it.

It is a teammate on the incident, not a pager. The responders stay in charge.

## What Prepcook does on an incident

1. **Picks it up.** The ticket pickup polls the incident tool for open
   incidents (triggered or acknowledged) on the configured services, created
   in the last day, and imports each one it has not seen into **Plan** on the
   board. The card's class is `investigation` unless `ticketing.type_mapping`
   maps `Incident` to something else; its title is the incident's, its
   description the triggering alert's body; urgency becomes the card's
   priority (`high` reads as P1, `low` as P3), and the service becomes an area
   tag when the board has a tag of that name. The card records where it came
   from: `origin` is `{ "kind": "incident", "ref": "<the incident's page>" }`.
2. **Acknowledges it.** When planning starts, a triggered incident is
   acknowledged as the configured Prepcook user. An incident a responder has
   already acknowledged (or resolved) is left as it is.
3. **Investigates.** Plan runs exactly as for any investigation: the app stood
   up, a reproduction harness, the `REPRODUCTION:` comment, the root-cause
   analysis, then the `TRIAGE REPORT` ([method.md](method.md#investigations)).
4. **Reports back.** When the card is triaged, the triage report is
   added to the incident as a note — what happens, where, why, scope,
   workaround and confidence — and `investigation_completed` goes to the
   notification targets ([notifications.md](notifications.md)).
5. **Resolves it, only if told to.** With
   `ticketing.incidents.resolve_on_triage = true` the incident is resolved
   once the report is on it. Otherwise it stays with the responders. A card
   converted to a bug and carried all the way to **Clean up** (the fix merged
   and the card closed) resolves its incident then.

Notes on the incident are its comments as far as Prepcook is concerned: a
plan decision (`RCA: APPROVED`, `RCA: CHANGES — …`, `REPLAN: …` as a note's
first line) from someone under **Plan approvers** works exactly as on a
ticket ([approvals.md](approvals.md#the-tracker-ticket)).

## What it never does

- **It never pages anyone.** It does not raise urgency, escalate, or add
  responders — adding one sends a responder request, which pages them, so it
  is not supported. The Prepcook user should be on no schedule and in no
  escalation policy.
- **It never resolves an incident on its own judgement.** Only
  `resolve_on_triage`, or a fix shipped through the board, resolves one.
- **It never takes an incident off its responders.** Planning acknowledges
  but does not reassign; the people on call keep it.
- **It never files incidents.** Pattern reviews and reopen follow-ups stay on
  the board in incidents mode, and the reopen and mention sweeps have nothing
  to do: incidents do not reopen, and notes carry no mentions.
- **It never retires a card because the incident was resolved.** Responders
  resolve once they have mitigated, usually before anyone knows why it
  happened; the investigation goes on.

## Setting up PagerDuty

**Settings → Integrations → Ticketing**: choose **Incidents**, add
**PagerDuty**, and fill in:

| Field | What to put there |
| --- | --- |
| API key (`pagerduty_api_token`) | A REST API key from **Integrations → API Access Keys**, able to read incidents and write notes. It is a secret: put it in the secrets store as `PAGERDUTY_API_TOKEN` (**Settings → Secrets**, or `prepcook secret set PAGERDUTY_API_TOKEN`) rather than in the config. |
| User id (`user_id`) | A dedicated PagerDuty user for Prepcook, e.g. `PABC123`, on no schedule and in no escalation policy. Notes and acknowledgements are written as this user (PagerDuty requires a `From` user for writes). Approvers are PagerDuty user ids. |
| Service ids (`service_ids`) | The services whose incidents Prepcook should pick up, comma-separated. Blank picks up every service the key can see. |
| Pickup filter (`pickup_filter`) | Optional terms narrowing the pickup: `service:PSVC1,PSVC2`, `urgency:high`, `status:triggered,acknowledged`, `assigned:@me`, `team:PTEAM1`. Default: `status:triggered,acknowledged`. |

**Test connection** reads the user, which proves the key and the user id.
Then turn on **Pick up new tickets automatically** under **Ticket pickup** and
choose the board incidents land on. A triaged incident's card stays in Plan
for a person ([method.md](method.md#investigations)); give the board tags
named after your services to have cards tagged with theirs.

The full field reference is in
[configuration.md](configuration.md#ticketing-providers).

## Without PagerDuty: the webhook alternative

Any incident tool, alerting rule or chat bot can open the same kind of card
without incidents mode, by calling the card API ([api.md](api.md#file-a-card)):

```sh
curl -sS -X POST "http://localhost:$PORT/api/cards" \
  -H "Authorization: Bearer $PREPCOOK_API_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "title": "Checkout returns 500s",
    "description": "Error rate 12% on /api/checkout since 09:00 UTC.",
    "work_class": "investigation",
    "origin": {
      "kind": "incident",
      "ref": "https://status.example.com/incidents/4821"
    },
    "start": true
  }'
```

The card is investigated the same way. The triage report then reaches the
filer through the `investigation_completed` event (a signed webhook target
gets the whole report, with `source.ref` naming the incident it was filed
for), since there is no incident in a configured tool to add a note to.
