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
  enamel-blue-hover-dark: '#4a7fbf'
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
  display:
    fontFamily: "Figtree, ui-sans-serif, system-ui, -apple-system, 'Segoe UI', Helvetica, Arial, sans-serif"
    fontSize: '50px'
    fontWeight: 700
    lineHeight: 1
    letterSpacing: '-1px'
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
  lead:
    fontFamily: 'Figtree, ui-sans-serif, system-ui, sans-serif'
    fontSize: '24px'
    fontWeight: 600
    lineHeight: 1.35
    letterSpacing: '-0.3px'
  headline:
    fontFamily: 'Figtree, ui-sans-serif, system-ui, sans-serif'
    fontSize: '32px'
    fontWeight: 700
    lineHeight: 1.3
    letterSpacing: '-0.5px'
  title-lg:
    fontFamily: 'Figtree, ui-sans-serif, system-ui, sans-serif'
    fontSize: '26px'
    fontWeight: 700
    lineHeight: 1.25
    letterSpacing: '-0.4px'
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
  section: '72px'
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
  card:
    backgroundColor: '{colors.surface}'
    textColor: '{colors.body-text}'
    rounded: '{rounded.lg}'
    padding: '20px'
  step-badge:
    backgroundColor: '{colors.enamel-blue-soft}'
    textColor: '{colors.enamel-blue-deep}'
    typography: '{typography.mono}'
    rounded: '{rounded.full}'
    size: '26px'
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
---

# Design System: Prepcook website

## Overview

**Creative North Star: "The pass"**

The pass is the counter between the kitchen and the dining room, where every
plate is checked before it goes out. The site is laid out like one: a clean,
lit counter (the warm-paper ground) with the work set down on it to be
inspected. The work is the product itself, shown in its own screenshots, its
own stage names, its own documentation and, on the homepage, its own board
drawn in code and running a simulated queue, and the site's job is to present
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
plus the app's status colours where the site draws the board.

### Primary

- **Enamel Blue** (`enamel-blue`): primary button fill, the brand mark, focus
  outlines, the hover edge on pager links and the table-of-contents rail. On
  the simulated board it is also the working dot, the lifted-card edge, the
  "You" cursor tag and the stage-track rule. The same value in both themes.
- **Deep Enamel** (`enamel-blue-deep`): primary button hover, link text, and
  the text on soft-blue fills in light mode. In dark mode its roles split:
  hover lightens to `enamel-blue-hover-dark`, link text to
  `enamel-blue-text-dark`.
- **Enamel Wash** (`enamel-blue-soft`): the current page in the docs sidebar,
  the numbered stage badges, the board's "PR open" chip and project tile, and
  text selection, always paired with Deep Enamel text
  (`enamel-blue-soft-fg-dark` in dark mode).

### Neutral

- **Paper** (`paper`): the page ground, the sticky header behind it, and the
  ground of the simulated app window.
- **Surface** (`surface`): cards, stage lists, install cards, tables, the
  inline table of contents, secondary buttons, code-block headers, and the
  board's cards and controls.
- **Panel** (`panel`): hover and selected fill for header links, secondary
  buttons and the board's controls, the active segment of the Chef/Prep
  control, and the inline-code chip.
- **Raised** (`raised`): code-block bodies and table header rows; a step
  quieter than Surface.
- **Border** (`border`): every 1px edge, section rules above doc `h2`s, the
  blockquote bar, the rules of the ruled lists, and the spent segments of the
  open-bugs bar.
- **Divider** (`divider`): hairlines inside a surface (between stages, table
  rows, under a code-block header).
- **Ink** (`ink`): headings, the wordmark, strong text, the lead pitch, board
  card titles and tally figures.
- **Body Text** (`body-text`): running text, the default.
- **Muted Text** (`muted-text`): labels, captions, helper lines, the footer,
  table-of-contents links, board column helpers and card keys.

### Status (inside the simulated board only)

Mirrored from the app so the drawn board reads exactly like the real one. The
stylesheet carries the amber tokens; the homepage adds green and red beside
them.

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

- **Display** (700, 50px, 1, -1px tracking): the wordmark hero of the current
  `src/home.html`, set in em so the mark scales with it; 38px under 640px. The
  board homepage has no wordmark hero, so this role leaves with that template.
- **Home Headline** (700, fluid, 1.02, tight tracking): the board homepage's
  one-line pitch, `h1`, balanced and held to 26ch.
- **Home Section** (700, fluid, 1.1, tight tracking): the board homepage's
  section headings, balanced and held to 22ch.
- **Lead** (600, 24px, 1.35): the one-sentence pitch under the wordmark on the
  current home page, in Ink, centred, 640px max; 20px under 640px.
- **Headline** (700, 32px, 1.3): a docs page's `h1`; 26px under 640px.
- **Title Large** (700, 26px, 1.25): band headings of the current home page.
- **Title** (700, 22px, 1.3): docs `h2`, ruled above with a Border line and
  24px of padding; 20px under 640px. Also the board header's project name.
- **Title Small** (700, 17px): card, stage and install-card headings; docs
  `h3` is 18px; stage-track and ruled-list headings run 18–19px.
- **Body** (400, 16px, 1.65): running text, docs measure 780px. The home
  problem statement is 17px, left-aligned in a 640px column; the board
  homepage's section ledes are 18px at 62ch.
- **Body Small** (400, 15px): text inside cards, stages, the stage track and
  ruled lists.
- **Control / Nav** (600 / 500, 14px): buttons, header links, sidebar links.
- **Label** (700, 11px, 0.08em, uppercase, Muted Text): navigation only, the
  docs-sidebar group titles and the docs breadcrumb.
- **Mono** (400, 13px, 1.6): code blocks; inline code is 0.86em on a Panel
  chip; stage numbers are 12px 500; board card keys are 12px.

### Named Rules

**The Heading Carries Itself Rule.** A section heading stands alone, with no
eyebrow, kicker or category label above it, and says something a reader can
check ("Nothing merges without a person"). The uppercase 11px label belongs to
navigation, the docs-sidebar group titles and the breadcrumb, where it names a
place rather than introducing a heading.

## Layout

- **Containers:** header, footer and docs at 1360px max; the current home page
  at 1120px; hand-written article pages at 820px; the docs article at 780px.
  Gutters are 24px, 16px under 640px.
- **Docs grid:** sidebar 248px, article, on-this-page 216px, 48px gaps. Under
  1200px the on-this-page column folds into an inline `details` block above
  the article and the sidebar narrows to 232px. Under 900px the sidebar
  becomes a slide-in drawer (300px or 86vw) over a scrim, opened from a menu
  button.
- **Home rhythm (current `src/home.html`):** hero padded 64px top, bands 72px
  apart, cards and gallery items on 16px and 24–32px gaps. Feature grids run
  three columns, two under 960px, one under 640px. Stage rows put the name in
  a 240px column beside the text, stacking under 960px.
- **Board homepage (`mock/home.html`):** the opening and the simulated board
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
- **Header:** sticky, 56px, Paper with a Border bottom edge; the scroll
  padding accounts for it.

## Elevation & Depth

The site is flat. No element casts a shadow. Depth is a matter of tone and
edge: Paper is the ground, Surface sits on it with a 1px Border, and Raised
and Panel tint regions inside a surface. Overlap happens only in the mobile
docs drawer, which sits over a scrim (`ink` at 45% in light, near-black at 60%
in dark) rather than casting a shadow, and in the simulated board, where the
"You" cursor and the "Queue clear" veil (Paper at 94%) lie over the columns.
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
items; 10px for cards, stage lists, screenshots, pager links and board cards;
12px for the outermost frames, the simulated app window and the screenshot
beside it on the board homepage, one step larger than the cards they hold;
5px for inline code chips; full rounding for stage badges, chips, pills and
tags. Inside a control the steps go smaller: 6px for the Chef/Prep segments
and the project tile, 2px for the open-bugs bar segments. Status and stage
dots are circles. Every container has a 1px border; screenshots get the same
border and a 10px corner (12px on the board homepage) so they sit on the page
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

### Cards / Containers

- **Corner Style:** 10px.
- **Background:** Surface on Paper.
- **Shadow Strategy:** none (see Elevation & Depth).
- **Border:** 1px Border.
- **Internal Padding:** 20px for feature and install cards, 20px × 24px for
  stage rows, 24px for the Tammy teaser.
- Link rows sit at the bottom of a feature card, pushed down so a row of cards
  aligns.
- The board homepage uses no feature cards; its sections use the stage track
  and ruled lists below. The card remains the container for the docs and the
  other pages.

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
  sideways rather than wrapping. On the board homepage the install command
  block sets its command at 16px.
- **Inline:** Panel chip, 5px corners, 0.86em, Ink.

### The stage list

The five stages of the method as one bordered Surface list, each row a
numbered Enamel Wash badge (26px, mono) beside the stage name and a one-line
helper, then the stage's description. Divider lines separate the rows. It
mirrors the board's left-to-right columns as a top-down reading order.

### The stage track

The board homepage's telling of the same five stages, left to right as the
board runs: five columns standing on a 2px Enamel Blue rule, each with a 12px
dot at its start, the stage name, one line in Deep Enamel 600 saying what the
stage proves, then a short paragraph. The last stage (Clean up) takes a Border
rule and a Muted Text dot. No boxes; the rule is the structure.

### Ruled lists

Where the board homepage lists things, it rules them rather than boxing them:
items separated by 1px Border lines top and bottom, 14–18px of vertical
padding, a heading or label column and the text. Used for the extra
capabilities, the quickstart steps (time in Deep Enamel 600 in a 170px column
beside the step), the integrations (a definition list with a 160px term
column) and the Tammy row. The mode notes beside the gate screenshot use a
1px left rule instead.

### Screenshot figure

Full-width product screenshots in light and dark pairs (`<picture>` switches
on `prefers-color-scheme`, and a script swaps the link target to the matching
full-size image), framed with the 1px Border and 10px corner, captioned in
13px Muted Text.

### The simulated board

The signature component of the board homepage: the app's own board drawn in
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

## Do's and Don'ts

### Do:

- **Do** take every colour from the custom properties in `site/src/site.css`
  (and, for the board status colours, the homepage's own mirror of them), and
  keep them identical to the board app's tokens.
- **Do** separate surfaces with a 1px Border and a tonal step; keep everything
  flat.
- **Do** show the product with its own screenshots, always as a light and dark
  pair, framed like a card.
- **Do** let a section heading carry itself: a sentence that says something,
  with nothing above it.
- **Do** label the simulated board as a simulation, pause it off screen, and
  start it paused under reduced motion.
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
