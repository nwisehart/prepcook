---
name: Prepcook website
description: The site at prepcook.dev, built from the repository's own docs, in the same flat, warm-paper system as the board app.
colors:
  enamel-blue: '#3f6aa6'
  enamel-blue-deep: '#2f5285'
  enamel-blue-soft: '#e7eef8'
  on-enamel: '#ffffff'
  paper: '#f7f6f3'
  surface: '#ffffff'
  panel: '#efede8'
  raised: '#fbfaf8'
  border: '#e4e1da'
  divider: '#f0eee9'
  ink: '#1f2328'
  body-text: '#4b4f56'
  muted-text: '#6a6f78'
  enamel-blue-hover-dark: '#4676b4'
  enamel-blue-text-dark: '#8fb0de'
  enamel-blue-soft-dark: '#243246'
  enamel-blue-soft-fg-dark: '#9dbce8'
  paper-dark: '#1e1e21'
  surface-dark: '#27282c'
  panel-dark: '#2d2e33'
  raised-dark: '#222225'
  border-dark: '#37383e'
  divider-dark: '#33343a'
  ink-dark: '#eceae4'
  body-text-dark: '#c9cbd0'
  muted-text-dark: '#a0a3aa'
  attention: '#d29a1c'
  attention-fg: '#9a6700'
  attention-soft: '#fff4d6'
  attention-border: '#ebd9a8'
  success: '#1e7b4c'
  success-soft: '#e3f3ea'
  error: '#9b2c1f'
  error-soft: '#fbe7e4'
  attention-dark: '#e7b94c'
  attention-fg-dark: '#e7b94c'
  attention-soft-dark: '#3a3120'
  attention-border-dark: '#5a4a1f'
  success-dark: '#5fc08a'
  success-soft-dark: '#1f3a2c'
  error-dark: '#f0a39b'
  error-soft-dark: '#432a28'
typography:
  home-headline:
    fontFamily: 'Figtree, ui-sans-serif, system-ui, sans-serif'
    fontSize: 'clamp(32px, 3.4vw, 50px)'
    fontWeight: 700
    lineHeight: 1.02
    letterSpacing: '-0.035em'
  home-section:
    fontFamily: 'Figtree, ui-sans-serif, system-ui, sans-serif'
    fontSize: 'clamp(28px, 3vw, 40px)'
    fontWeight: 700
    lineHeight: 1.1
    letterSpacing: '-0.03em'
  headline:
    fontFamily: 'Figtree, ui-sans-serif, system-ui, sans-serif'
    fontSize: '32px'
    fontWeight: 700
    lineHeight: 1.3
    letterSpacing: '-0.5px'
  title:
    fontFamily: 'Figtree, ui-sans-serif, system-ui, sans-serif'
    fontSize: '22px'
    fontWeight: 700
    lineHeight: 1.3
    letterSpacing: '-0.3px'
  title-sm:
    fontFamily: 'Figtree, ui-sans-serif, system-ui, sans-serif'
    fontSize: '17px'
    fontWeight: 700
    lineHeight: 1.3
  body:
    fontFamily: 'Figtree, ui-sans-serif, system-ui, sans-serif'
    fontSize: '16px'
    fontWeight: 400
    lineHeight: 1.65
  body-sm:
    fontFamily: 'Figtree, ui-sans-serif, system-ui, sans-serif'
    fontSize: '15px'
    fontWeight: 400
    lineHeight: 1.65
  control:
    fontFamily: 'Figtree, ui-sans-serif, system-ui, sans-serif'
    fontSize: '14px'
    fontWeight: 600
  nav:
    fontFamily: 'Figtree, ui-sans-serif, system-ui, sans-serif'
    fontSize: '14px'
    fontWeight: 500
    lineHeight: 1.4
  label:
    fontFamily: 'Figtree, ui-sans-serif, system-ui, sans-serif'
    fontSize: '11px'
    fontWeight: 700
    letterSpacing: '0.08em'
  mono:
    fontFamily: "'JetBrains Mono', ui-monospace, SFMono-Regular, Menlo, Consolas, monospace"
    fontSize: '13px'
    fontWeight: 400
    lineHeight: 1.6
rounded:
  inline: '5px'
  md: '8px'
  lg: '10px'
  window: '12px'
  full: '999px'
spacing:
  xs: '4px'
  sm: '8px'
  md: '12px'
  lg: '16px'
  xl: '24px'
  2xl: '32px'
  3xl: '48px'
  section: '112px'
components:
  button-primary:
    backgroundColor: '{colors.enamel-blue}'
    textColor: '{colors.on-enamel}'
    typography: '{typography.control}'
    rounded: '{rounded.md}'
    padding: '0 16px'
    height: '40px'
  button-primary-hover:
    backgroundColor: '{colors.enamel-blue-deep}'
  button-secondary:
    backgroundColor: '{colors.surface}'
    textColor: '{colors.ink}'
    typography: '{typography.control}'
    rounded: '{rounded.md}'
    padding: '0 16px'
    height: '40px'
  button-secondary-hover:
    backgroundColor: '{colors.panel}'
  icon-button:
    backgroundColor: '{colors.surface}'
    textColor: '{colors.ink}'
    rounded: '{rounded.md}'
    size: '36px'
  header-link:
    textColor: '{colors.body-text}'
    typography: '{typography.nav}'
    rounded: '{rounded.md}'
    padding: '6px 10px'
  header-link-active:
    backgroundColor: '{colors.panel}'
    textColor: '{colors.ink}'
  sidebar-link-active:
    backgroundColor: '{colors.enamel-blue-soft}'
    textColor: '{colors.enamel-blue-deep}'
    rounded: '{rounded.md}'
    padding: '5px 10px'
  code-inline:
    backgroundColor: '{colors.panel}'
    textColor: '{colors.ink}'
    rounded: '{rounded.inline}'
    padding: '1px 5px'
  code-block:
    backgroundColor: '{colors.raised}'
    textColor: '{colors.ink}'
    typography: '{typography.mono}'
    rounded: '{rounded.md}'
    padding: '12px 14px'
  app-window:
    backgroundColor: '{colors.paper}'
    rounded: '{rounded.window}'
  board-card:
    backgroundColor: '{colors.surface}'
    textColor: '{colors.ink}'
    rounded: '{rounded.lg}'
    padding: '11px 14px 12px'
  board-card-needs-you-strip:
    backgroundColor: '{colors.attention-soft}'
    textColor: '{colors.attention-fg}'
    padding: '7px 14px'
  needs-you-pill:
    backgroundColor: '{colors.attention-soft}'
    textColor: '{colors.attention-fg}'
    rounded: '{rounded.full}'
    padding: '0 10px'
    height: '24px'
  chip-bug:
    backgroundColor: '{colors.error-soft}'
    textColor: '{colors.error}'
    rounded: '{rounded.full}'
    padding: '0 7px'
    height: '20px'
  chip-merged:
    backgroundColor: '{colors.success-soft}'
    textColor: '{colors.success}'
    rounded: '{rounded.full}'
    padding: '0 7px'
    height: '20px'
  chip-pr:
    backgroundColor: '{colors.enamel-blue-soft}'
    textColor: '{colors.enamel-blue-deep}'
    rounded: '{rounded.full}'
    padding: '0 7px'
    height: '20px'
  sim-tag:
    textColor: '{colors.muted-text}'
    rounded: '{rounded.full}'
    padding: '3px 9px'
  sim-segmented:
    backgroundColor: '{colors.surface}'
    rounded: '{rounded.md}'
    padding: '2px'
  sim-segmented-active:
    backgroundColor: '{colors.panel}'
    textColor: '{colors.ink}'
    height: '30px'
  chip-investigation:
    backgroundColor: '{colors.panel}'
    textColor: '{colors.body-text}'
    rounded: '{rounded.full}'
    padding: '0 7px'
    height: '20px'
  switch:
    backgroundColor: '{colors.border}'
    rounded: '{rounded.full}'
    width: '34px'
    height: '20px'
  switch-on:
    backgroundColor: '{colors.enamel-blue}'
  pane-header:
    textColor: '{colors.ink}'
    padding: '0 20px'
    height: '46px'
  chat-avatar-customer:
    backgroundColor: '{colors.panel}'
    textColor: '{colors.ink}'
    rounded: '{rounded.md}'
    size: '36px'
  chat-avatar-teammate:
    backgroundColor: '{colors.enamel-blue-soft}'
    textColor: '{colors.enamel-blue-deep}'
    rounded: '{rounded.md}'
    size: '36px'
  chat-avatar-tammy:
    backgroundColor: '{colors.paper}'
    rounded: '{rounded.md}'
    size: '36px'
  tool-chip:
    backgroundColor: '{colors.panel}'
    textColor: '{colors.ink}'
    rounded: '{rounded.full}'
    padding: '0 8px'
    height: '22px'
  triage-report:
    backgroundColor: '{colors.raised}'
    textColor: '{colors.body-text}'
    rounded: '{rounded.md}'
    padding: '9px 10px'
---

# Design System: Prepcook website

## Overview

**Creative North Star: "The pass"**

The pass is the counter between the kitchen and the dining room, where every
plate is checked before it goes out. The site is laid out like one: a clean,
lit counter (the warm-paper ground) with the work set down on it to be
inspected. The work is the product itself, shown in its own screenshots, its
own stage names, its own documentation and, on the homepage and the Tammy page, its own
board drawn in code and running a simulated queue (on the Tammy page beside
a customer's conversation), and the site's job is to present
it plainly enough that a lead can check it and sign off.

The system is the same one the board app uses, so the site and the product
read as one object. It is flat and border-defined: surfaces are white panels
with a 1px warm-grey edge, nothing is raised by a shadow, and a single enamel
blue marks what can be acted on. Density is that of good documentation:
generous line height, a 640–780px reading measure, and headings that state a
claim on their own rather than waiting under a label. Light and dark are both
first-class; the dark palette is a chosen set of warm near-blacks, not an
inversion.

The kitchen appears in the words and the mark (a pleated toque) and may appear
visually only in sparing, restrained nods that earn their place. The site must
never become a dark hacker hero: no black-first terminal aesthetic, no neon,
no animated code.

**Key Characteristics:**

- Warm paper ground, white bordered surfaces, no shadows.
- One accent, enamel blue, for actions, links, the mark and the current page.
- Figtree for everything readable, JetBrains Mono for keys, paths and code.
- The product carries the visual weight: screenshots in light and dark pairs,
  and on the homepage the simulated board itself.
- Quiet and exact: thin borders, flat fills, precise spacing.

## Colors

A warm, low-chroma neutral world with a single blue accent, mirrored token for
token from the board app (`packages/web-core/src/app/styles/new/index.css`),
plus the app's status colours where the site draws the board. One value
departs from the app: the dark-mode primary hover (`enamel-blue-hover-dark`)
is a step darker than the app's #4A7FBF so white button text on it clears
4.5:1 (4.66:1). Every other value still mirrors the app.

### Primary

- **Enamel Blue** (`enamel-blue`): primary button fill, the brand mark, focus
  outlines, the hover edge on pager links and the table-of-contents rail. On
  the simulated board it is also the working dot, the lifted-card edge, the
  "You" cursor tag and the stage-track rule. On the Tammy page it is the dot
  on each tool-call chip and the Show her tools switch when on. The same value
  in both themes.
- **Deep Enamel** (`enamel-blue-deep`): primary button hover, link text, and
  the text on soft-blue fills in light mode. In dark mode its roles split:
  hover lightens to `enamel-blue-hover-dark` (the contrast exception above),
  link text to `enamel-blue-text-dark`.
- **Enamel Wash** (`enamel-blue-soft`): the current page in the docs sidebar,
  the board's "PR open" chip and project tile, a
  teammate's avatar in the chat thread, and text selection, always paired with Deep Enamel text
  (`enamel-blue-soft-fg-dark` in dark mode).

### Neutral

- **Paper** (`paper`): the page ground, the sticky header behind it, the
  ground of the simulated app window and the conversation frame, and the tile
  behind Tammy's mark in her avatar.
- **Surface** (`surface`): tables, the inline table of contents, pager
  links, secondary and icon buttons, code-block headers, the backing of the
  homepage's gate screenshot, and the board's cards and controls; the thread pane and team-channel pane of the
  conversation frame.
- **Panel** (`panel`): hover and selected fill for header links, secondary
  buttons and the board's controls, the active segment of the Chef/Prep
  control, the inline-code chip, the investigation chip, and in the chat thread
  the customer's avatar, the tool-call chips and the "App" tag.
- **Raised** (`raised`): code-block bodies, table header rows and the triage
  report on a board card; a step quieter than Surface.
- **Border** (`border`): every 1px edge, section rules above doc `h2`s, the
  blockquote bar, the rules of the ruled lists, and the spent segments of the
  open-bugs bar.
- **Divider** (`divider`): hairlines inside a surface (between table rows,
  under a code-block header) and the edge of the triage report.
- **Ink** (`ink`): headings, the header name, strong text, board card titles
  and tally figures.
- **Body Text** (`body-text`): running text, the default.
- **Muted Text** (`muted-text`): labels, captions, helper lines, the footer,
  table-of-contents links, board column helpers and card keys, message
  times, citation lines, pane-header notes and the empty-pane line.

### Status (inside the simulated board only)

Mirrored from the app so the drawn board reads exactly like the real one. The
shared stylesheet carries all of them, amber, green and red, with their dark
values; only the homepage and the Tammy page draw with them.

- **Needs-You Amber** (`attention`, `attention-fg`, `attention-soft`,
  `attention-border`): the dot, text, fill and edge of a card that needs a
  person, and of the Needs-you pill in the board header.
- **Merged Green** (`success`, `success-soft`): the Shipped column dot, the
  "Merged" chip, done status lines, the Merged tally and the "Board up" dot.
- **Bug Red** (`error`, `error-soft`): the "bug" chip only.

Each neutral and status colour has a `-dark` counterpart in the frontmatter,
applied under `prefers-color-scheme: dark` unless the reader picked light, and
under `[data-theme='dark']` when they picked dark.

### Named Rules

**The One Accent Rule.** Enamel blue is the only accent on the site's own
surfaces. Status colour (amber, green, red) appears only inside the simulated
board and in product screenshots, where it means what it means in the app.

**The Needs-You Amber Rule.** Amber appears only inside the simulated board,
and only where a person is needed: a card's Needs-you strip and edge, its
amber dot, the Needs-you pill. It never decorates a page, a heading or a call
to action.

## Typography

**Body Font:** Figtree (with ui-sans-serif, system-ui, -apple-system, Segoe
UI, Helvetica, Arial)
**Label/Mono Font:** JetBrains Mono (with ui-monospace, SFMono-Regular,
Menlo, Consolas)

**Character:** One friendly, open grotesque does every job from the headline
to the footer, tightened with negative tracking as it grows; the mono is kept
for things a reader would copy or type.

### Hierarchy

- **Home Headline** (700, fluid, 1.02, tight tracking): the homepage's
  one-line pitch, `h1`, balanced and held to 26ch; the Tammy page's two-line
  pitch at 24ch under her wordmark.
- **Home Section** (700, fluid, 1.1, tight tracking): section headings on the
  homepage and the Tammy page, balanced and held to 22ch.
- **Headline** (700, 32px, 1.3): a docs page's `h1`; 26px under 640px.
- **Title** (700, 22px, 1.3): docs `h2`, ruled above with a Border line and
  24px of padding; 20px under 640px. Also the board header's project name.
- **Title Small** (700, 17px): the "Prepcook" name in the simulated app's
  sidebar; docs `h3` is 18px; stage-track and ruled-list headings run
  18–19px.
- **Body** (400, 16px, 1.65): running text, docs measure 780px. The
  homepage's opening paragraph under the headline is 17px at 66ch; the
  homepage's and Tammy page's section ledes are 18px at 62ch (16px under
  640px), and the lede under the conversation is 18px Ink at 68ch.
- **Body Small** (400, 15px): text in the stage track, the ruled lists and
  the mode notes. Chat message bodies use it at 1.55 line height, capped at
  60ch (14.5px under 640px; 13.5px in the team-channel pane); the "never"
  list sets its items at 18px Ink (16px under 640px).
- **Control / Nav** (600 / 500, 14px): buttons, header links, sidebar links.
- **Label** (700, 11px, 0.08em, uppercase, Muted Text): navigation only, the
  docs-sidebar group titles and the docs breadcrumb.
- **Mono** (400, 13px, 1.6): code blocks; inline code is 0.86em on a Panel
  chip; board card keys are 12px; tool-call chips
  are 11.5px; the triage report's label is 11px 500.

### Named Rules

**The Heading Carries Itself Rule.** A section heading stands alone, with no
eyebrow, kicker or category label above it, and says something a reader can
check ("Nothing merges without a person"). The uppercase 11px label belongs to
navigation, the docs-sidebar group titles and the breadcrumb, where it names a
place rather than introducing a heading.

## Layout

- **Containers:** header, footer and docs at 1360px max; the docs article at
  780px; the homepage and Tammy page openings at 1440px and their sections at
  1200px (below). Gutters are 24px, 16px under 640px.
- **Docs grid:** sidebar 248px, article, on-this-page 216px, 48px gaps. Under
  1200px the on-this-page column folds into an inline `details` block above
  the article and the sidebar narrows to 232px. Under 900px the sidebar
  becomes a slide-in drawer (300px or 86vw) over a scrim, opened from a menu
  button.
- **Homepage (`src/home.html`, `src/home.css`):** the opening and the simulated board
  run to 1440px; the pitch sits beside a 340–400px action column (the command,
  Get started, a one-line note) in a short band of about 220px, and the app
  window fills the rest of the first viewport (its height tracks the viewport
  between 540px and 700px; 560px under 900px). Sections below sit at 1200px
  with 40px gutters, 112px apart (80px and 16px gutters under 640px), and
  split unevenly (5:7 for the gate, 7:5 for the knowledge band) rather than
  into equal boxes. The app sidebar is 232px, 192px under 1360px, and hidden
  under 900px; the five columns hold at least 176px each and scroll sideways
  at a fixed 244px under 640px. Everything stacks under 1100px; the stage
  track goes two-up there and one-up under 640px.
- **Tammy page (`src/tammy.html`, `src/tammy.css`):** the same 1440px opening
  and 1200px sections as the homepage. The opening is Tammy's wordmark (168px
  wide, 132px under 640px) over the headline, beside the action row (Set her up,
  GitHub, a one-line note). Below it the conversation frame splits 7:5, the
  customer's thread beside the kitchen, at a viewport-tracking height between
  640px and 700px; the kitchen stacks a two-column board (3:2, Plan and
  Execute) over a 150px team-channel pane (170px under 640px). Under 900px the
  frame stacks: the thread at 460px, then the kitchen at its natural height
  with the board two-up. Content splits run 5:7 (heading and lede beside a
  ruled list) and the close runs text beside a 320–420px action column; both
  stack under 1100px, where the four-job track goes two-up (one-up under
  640px).
- **Header:** sticky, 56px, Paper with a Border bottom edge; the scroll
  padding accounts for it.

## Elevation & Depth

The site is flat. No element casts a shadow. Depth is a matter of tone and
edge: Paper is the ground, Surface sits on it with a 1px Border, and Raised
and Panel tint regions inside a surface. Overlap happens only in the mobile
docs drawer, which sits over a scrim (`ink` at 45% in light, near-black at 60%
in dark) rather than casting a shadow, and in the simulated board, where the
"You" cursor and the "Queue clear" veil (Paper at 94%) lie over the columns;
on the Tammy page a Paper veil covers the conversation frame between runs.
The one use of `box-shadow` is as a flat ring: a 3px Paper halo that cuts each
stage-track dot out of its rule, and a 2px inset Muted Text ring that draws the
hollow To Do dot.

### Named Rules

**The Flat Counter Rule.** Surfaces are separated by borders and tone, never
by shadow or blur. If something needs to stand out, give it the accent, not
height: a card being moved by the cursor turns its edge Enamel Blue instead
of lifting.

## Shapes

Gentle, consistent corners: 8px for controls, code blocks, tables and nav
items; 10px for docs screenshots, pager links and board cards;
12px for the outermost frames, the simulated app window and the gate
screenshot beside it on the homepage, and the Tammy page's conversation frame, one
step larger than the cards they hold; 8px for chat avatars and the triage
report;
5px for inline code chips; full rounding for chips, pills,
tags, tool-call chips and switches. Inside a control the steps go smaller: 6px for the Chef/Prep segments
and the project tile, 2px for the open-bugs bar segments. Status and stage
dots are circles. Every container has a 1px border; screenshots get the same
border and a 10px corner (12px on the homepage) so they sit on the page
like cards. The focus outline is 2px Enamel Blue, offset 2px, with a 4px
corner.

## Components

Quiet and exact: thin borders, flat fills, nothing raised.

### Buttons

- **Shape:** gently rounded (8px), at least 40px tall, 16px side padding,
  14px Figtree 600.
- **Primary:** Enamel Blue fill and border, white text ("Get started").
- **Hover / Focus:** Primary deepens to Deep Enamel; secondary takes the
  Panel fill. Focus shows the 2px Enamel Blue outline. No transitions.
- **Secondary:** Surface fill, Border edge, Ink text ("GitHub").
- **Icon button:** 36px square, Surface fill, Border edge; used for the theme
  toggle and the mobile menu.

### Containers

- **Corner Style:** 8px for code blocks, tables and the inline table of
  contents; 10px for pager links and board cards.
- **Background:** Surface on Paper.
- **Shadow Strategy:** none (see Elevation & Depth).
- **Border:** 1px Border.
- The site has no freestanding content card. The docs box only code, tables,
  the inline table of contents and the pager; the homepage and the Tammy page
  rule their lists instead of boxing them (see Ruled lists).

### Navigation

- **Header:** wordmark (28px mark in Enamel Blue, "Prepcook" 18px 700 Ink) on
  the left; Docs, Tammy and GitHub as 14px 500 links that take a Panel fill on
  hover and when current; the theme toggle at the end. Under 480px the
  external GitHub link is dropped.
- **Docs sidebar:** uppercase group labels, 14px 500 links; the current page
  takes the Enamel Wash fill with Deep Enamel 600 text.
- **On this page:** 13px Muted Text links on a 2px Border rail whose segment
  turns Enamel Blue on hover.
- **Pager:** previous and next as bordered 10px cards, label above title;
  the border turns Enamel Blue on hover.

### Code

- **Block:** Raised body in a bordered 8px container, with a Surface header
  strip carrying the language in 12px mono and a ghost Copy button. Scrolls
  sideways rather than wrapping. On the homepage the install command blocks
  (the opening and the close) set the command at 16px.
- **Inline:** Panel chip, 5px corners, 0.86em, Ink.

### The stage track

The homepage's telling of the method's five stages, left to right as the
board runs: five columns standing on a 2px Enamel Blue rule, each with a 12px
dot at its start, the stage name, one line in Deep Enamel 600 saying what the
stage proves, then a short paragraph. The last stage (Clean up) takes a Border
rule and a Muted Text dot. No boxes; the rule is the structure.

### Ruled lists

Where the homepage lists things, it rules them rather than boxing them:
items separated by 1px Border lines top and bottom, 14–18px of vertical
padding, a heading or label column and the text. Used for the extra
capabilities, the quickstart steps (time in Deep Enamel 600 in a 170px column
beside the step), the integrations (a definition list with a 160px term
column) and the Tammy row. The mode notes beside the gate screenshot use a
1px left rule instead.

On the Tammy page the same form carries three more lists:

- **The "never" list:** the things she never does, each item 18px Ink on a
  16px row, marked by a short 14px × 2px Enamel Blue dash in a 28px column
  rather than a bullet or a number.
- **The seats list:** a definition list, each term (700 Ink, with its config
  value in inline code beneath in Muted Text) in a 220px column beside its
  description; the term stacks over the description under 640px.
- **The link list:** the setup documents, each a 600 link followed by what it
  covers, on 14px rows.

### Tables

A bordered Surface container at 8px with a Raised header row (600 Ink) and
Divider rules between rows; cells 8px × 12px at 14px. It scrolls sideways
inside its frame when wide. On the Tammy page the tools table restacks under
640px: the header row hides and each row becomes a block (12px × 14px,
Divider below), the tool name over its description, with the "Offered for"
cell in 13px Muted Text.

### Screenshot figure

Product screenshots in light and dark pairs (`<picture>` switches on
`prefers-color-scheme`, the script forces the matching source when the reader
picks a theme and swaps any full-size link to the matching image), framed with
the 1px Border. In the docs they take a 10px corner; the homepage's gate
screenshot takes the 12px frame corner on Surface and is captioned in 13px
Muted Text.

### The simulated board

The signature component of the homepage: the app's own board drawn in
code at full size, labelled as a simulation, running the Pantry demo's bugs
through the real stage rules.

- **App window:** a 12px, 1px-bordered Paper frame. Its sidebar carries the
  mark and "Prepcook", the Pantry project in a bordered Surface row with an
  Enamel Wash tile, the Board / Sessions / Knowledge / Settings items (Board
  current, Panel fill, Enamel Blue icon), a bordered mode card with the
  Chef/Prep switch (Border track, Enamel Blue when on) and a footer with the
  version in mono and a green "Board up" dot. The main pane has a 42px crumb
  bar ruled below, a board header (the project name in Title, the amber
  Needs-you pill with its count, a muted card count) and five columns: To Do
  (hollow dot), Plan, Execute, Review (Enamel Blue dots) and Shipped (green
  dot), each headed by its name, count and a 12px helper line.
- **Cards:** Surface, 1px Border, 10px corners. A key in 12px mono and chips
  (bug in Bug Red, investigation in Panel, PR open in Enamel Wash, Merged in
  Merged Green) top the card, then a 13.5px 600 title and one status line:
  working in Deep Enamel with a breathing Enamel Blue dot, idle in Muted Text
  with a clock icon, done in Merged Green with a green dot, needs-you in amber.
  A card that needs a person takes the Attention Border edge and an amber
  strip across its top ("Needs you · the RCA is ready", "Merged — close
  card"), plus flat 30px action buttons (Approve / Review, Convert to bug /
  Close, Clean up), the first in Enamel Blue.
- **The "You" cursor:** an Ink arrow with a Surface outline and a small
  Enamel Blue "You" tag. It travels to the card that needs a person, presses
  its button (the button scales to 0.96 and deepens), drags cards between
  columns in Chef mode, and rests at 55% opacity between tasks.
- **The sim bar:** above the window, right-aligned. A bordered "Simulated run
  on the Pantry demo" tag; the Open bugs tally (Ink 26px 700 tabular figure
  beside a bar of 6 × 14px Enamel Blue segments that turn Border as bugs
  close); the Merged tally (figure in Merged Green); a Chef mode / Prep mode
  segmented control (Surface, Border edge, the active segment in Panel); and
  a bordered Pause / Play button with its icon. Under 640px the bar wraps
  left-aligned and the tag moves to the end.
- **Motion:** after each render every card plays from where it was to where it
  is (FLIP) on the long-tail ease-out, 720ms across columns and 420ms within
  one; a new card drops in 10px as it fades up over 500ms. The cursor glides
  on its own ease-in-out. A "Queue clear" veil fades over the columns when the
  queue empties, and the run starts again.
- **Pausing:** the simulation clock advances only while the board is playing
  and on screen; it pauses when the window scrolls out of view or the tab is
  hidden, and every running animation in the window pauses with it. Under
  `prefers-reduced-motion` it starts paused, cards jump instead of gliding and
  the working dot stops breathing; Play still runs it.
- **Accessibility:** the columns are hidden from assistive technology and a
  visually hidden paragraph describes the run; the tallies, segmented control
  and Pause are real, labelled controls.

### The chat thread

The Tammy page's signature component: one customer conversation played live
beside the kitchen. It is a generic chat thread in Prepcook's own look, never
a copy of Slack's interface or branding, and it is labelled as a simulation.

- **Conversation frame:** a 12px, 1px-bordered Paper frame split 7:5. The
  thread pane on the left is Surface with a Border right edge; the kitchen on
  the right holds a two-column board over the team-channel pane (Surface,
  Border top edge). Under 900px the panes stack (see Layout).
- **Pane headers:** 46px tall (40px and 13px in the team pane), 20px side
  padding, ruled below with Border; the channel or board name in 14px 700
  Ink beside a 12px 500 Muted Text note ("Pantry's support channel in
  Slack"). The board's header carries the amber Needs-you pill while a card
  waits for a person.
- **Messages:** a 36px avatar column beside the body (28px in the team
  pane), 8px corners. The customer's avatar is Panel with an Ink initial, a
  teammate's is Enamel Wash with a Deep Enamel initial, and Tammy's is her
  mark on Paper with a 1px Border. Above the body, the name in 14px 700 Ink,
  Tammy's small Panel "App" tag in 11px 600 Muted Text, and the time in 12px
  500 Muted Text with tabular figures. Bodies run to 60ch; numbered questions
  are an ordinary ordered list.
- **Citation line:** under an answer, an italic 13px Muted Text line led by a
  14px stroked book icon, naming the passage she answered from.
- **Tool-call chips:** under a message, the tools she called as full-round
  Panel chips (22px) in mono Ink with a 6px Enamel Blue dot. They show only
  on this page, never to the customer, and the Show her tools switch hides
  them all.
- **Typing indicator:** Tammy's avatar and name over three 6px Muted Text
  dots that pulse in turn (1.2s, staggered 0.2s); under reduced motion they
  hold still.
- **Empty pane:** until she posts, the team pane shows one centred 13px Muted
  Text line saying what will appear there.
- **The board card it feeds:** the same card as the simulated board, with
  the investigation chip (Panel), an idle "Filed by Tammy" line, a working
  line while it is investigated, and on triage the Needs-you strip and edge
  plus the triage report: a Raised block with a Divider edge and 8px corners,
  the label TRIAGE REPORT in 11px 500 mono Ink, then a definition grid (What,
  Where, Fix for now, Confidence) with terms in Muted Text, at 12px, above the
  Convert to bug / Close actions.
- **The sim bar:** above the frame, as on the homepage: the bordered
  "Simulated conversation on the Pantry demo" tag on the left, then the Show
  her tools switch (a 34 × 20px full-round track, Border when off and Enamel
  Blue when on, with a Surface knob that slides 14px; 13px 600 Ink label; a
  real `role="switch"`) and the bordered Pause / Play button.
- **Motion:** each message rises 6px as it fades in over 360ms on the
  long-tail ease-out; the card drops in and moves between columns as on the
  simulated board. The thread scrolls inside its own pane to the newest
  message, and when the board is shorter than the triaged card it scrolls
  inside itself to the card's actions. A Paper veil fades over the frame at
  the end of the run, and the conversation starts again.
- **Pausing:** the script runs on a simulation clock that advances only while
  playing and on screen; it pauses when the frame scrolls out of view or the
  tab is hidden, and every running animation in the frame pauses with it.
  Under `prefers-reduced-motion` the whole conversation is shown at once,
  paused, with nothing animating; Play replays it from the start.

## Do's and Don'ts

### Do:

- **Do** take every colour, the board status colours included, from the
  custom properties in `site/src/site.css`, and keep them identical to the board app's tokens, except the dark-mode primary
  hover, which is darker for contrast.
- **Do** separate surfaces with a 1px Border and a tonal step; keep everything
  flat.
- **Do** show the product with its own screenshots, always as a light and dark
  pair, framed with the 1px Border.
- **Do** let a section heading carry itself: a sentence that says something,
  with nothing above it.
- **Do** label every simulation (the board, the conversation) as a
  simulation, pause it off screen, and under reduced motion start it paused:
  the board at its first state, the conversation shown whole.
- **Do** set keys, paths, commands and versions in JetBrains Mono.
- **Do** design both themes at once; dark is a warm near-black palette, not an
  inversion.
- **Do** keep any kitchen imagery to a sparing, restrained nod that earns its
  place; the toque mark is the anchor.

### Don't:

- **Don't** add shadows, glows or blur to lift a surface.
- **Don't** introduce a second accent hue, and don't use amber, green or red
  outside the simulated board; amber always means a person is needed.
- **Don't** put an eyebrow, kicker or uppercase category label above a
  heading; the uppercase label is for navigation.
- **Don't** turn the site into a dark hacker hero: no black-first terminal
  aesthetic, no neon green, no animated code.
- **Don't** use emoji or exclamation marks, in copy or as icons.
