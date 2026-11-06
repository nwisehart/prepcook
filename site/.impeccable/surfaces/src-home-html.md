---
version: 1
slug: "src-home-html"
primary_target: "src/home.html"
related_targets: ["mock/home.html"]
---

# Surface brief: homepage

Scope: the prepcook.dev homepage (`site/src/home.html`), mocked as a standalone page at `site/mock/home.html` before the real template changes. Mode: Persuade, with the product leading the first viewport as it would on an Experience surface.

Audience and job: the team lead deciding whether to adopt Prepcook. They must see the method working, believe a person keeps the final say, and reach the Pantry quickstart.

Constraints: the established system in DESIGN.md. The simulated board brings in the app's own status tokens (success, error and their soft fills), the 12px app-window corner, and larger fluid display and section headings; DESIGN.md records them once the build is reviewed. Simulated board data is the Pantry demo's real bugs, labelled as a simulation. No invented metrics; the queue counter belongs to the simulation only. New copy is allowed from README facts, listed for approval.

## Direction contract

THESIS: The homepage is the board at work, not a description of it. It refuses the centred-wordmark-over-paragraphs hero and the row of feature boxes.

OWN-WORLD: The app's own board, drawn in code at full size: warm paper ground, white 10px cards with 1px borders, enamel-blue working dots, amber Needs-you strips, green Merged chips, Figtree with mono keys. No shadows.

STORY: A lead watches cards cross Plan, Execute, Review and Shipped, sees the run stop at the amber gate until someone approves, watches the open-bug count fall, flips Chef to Prep mode and sees approvals come from the ticket instead, then installs.

FIRST VIEWPORT: A one-line pitch and the `npx prepcook up` command with Get started across the top 220px; beneath, a live app window (sidebar, Pantry header with the open-bug counter and Needs-you pill, five columns) filling the rest edge to edge. Signature interaction: the simulation, cards moving between columns with FLIP motion, a cursor approving at the gate, a working Chef/Prep switch, pause control, reduced-motion static state.

FORM: User-pinned live-board showcase (steer over the dealt hand; nearest grounded structure: "The 20-minute tour", position 5). seed:3b0438006860ee8d

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
