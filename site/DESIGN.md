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
typography:
  display:
    fontFamily: "Figtree, ui-sans-serif, system-ui, -apple-system, 'Segoe UI', Helvetica, Arial, sans-serif"
    fontSize: '50px'
    fontWeight: 700
    lineHeight: 1
    letterSpacing: '-1px'
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
---

# Design System: Prepcook website

## Overview

**Creative North Star: "The pass"**

The pass is the counter between the kitchen and the dining room, where every
plate is checked before it goes out. The site is laid out like one: a clean,
lit counter (the warm-paper ground) with the work set down on it to be
inspected. The work is the product itself, shown in its own screenshots, its
own stage names and its own documentation, and the site's job is to present it
plainly enough that a lead can check it and sign off.

The system is the same one the board app uses, so the site and the product
read as one object. It is flat and border-defined: surfaces are white panels
with a 1px warm-grey edge, nothing is raised by a shadow, and a single enamel
blue marks what can be acted on. Density is that of good documentation:
generous line height, a 640–780px reading measure, small uppercase labels that
name each section before its heading. Light and dark are both first-class; the
dark palette is a chosen set of warm near-blacks, not an inversion.

The kitchen appears in the words and the mark (a pleated toque) and may appear
visually only in sparing, restrained nods that earn their place. The site must
never become a dark hacker hero: no black-first terminal aesthetic, no neon,
no animated code.

**Key Characteristics:**

- Warm paper ground, white bordered surfaces, no shadows.
- One accent, enamel blue, for actions, links, the mark and the current page.
- Figtree for everything readable, JetBrains Mono for keys, paths and code.
- Product screenshots, in light and dark pairs, carry the visual weight.
- Quiet and exact: thin borders, flat fills, precise spacing.

## Colors

A warm, low-chroma neutral world with a single blue accent, mirrored token for
token from the board app (`packages/web-core/src/app/styles/new/index.css`).

### Primary

- **Enamel Blue** (`enamel-blue`): primary button fill, the brand mark, focus
  outlines, the hover edge on pager links and the table-of-contents rail. The
  same value in both themes.
- **Deep Enamel** (`enamel-blue-deep`): primary button hover, link text, and
  the text on soft-blue fills in light mode. In dark mode its roles split:
  hover lightens to `enamel-blue-hover-dark`, link text to
  `enamel-blue-text-dark`.
- **Enamel Wash** (`enamel-blue-soft`): the current page in the docs sidebar
  and the numbered stage badges, always paired with Deep Enamel text
  (`enamel-blue-soft-fg-dark` in dark mode).

### Neutral

- **Paper** (`paper`): the page ground and the sticky header behind it.
- **Surface** (`surface`): cards, stage lists, install cards, tables, the
  inline table of contents, secondary buttons and code-block headers.
- **Panel** (`panel`): hover and selected fill for header links and secondary
  buttons, and the inline-code chip.
- **Raised** (`raised`): code-block bodies and table header rows; a step
  quieter than Surface.
- **Border** (`border`): every 1px edge, section rules above doc `h2`s, the
  blockquote bar.
- **Divider** (`divider`): hairlines inside a surface (between stages, table
  rows, under a code-block header).
- **Ink** (`ink`): headings, the wordmark, strong text, the lead pitch.
- **Body Text** (`body-text`): running text, the default.
- **Muted Text** (`muted-text`): labels, captions, helper lines, the footer,
  table-of-contents links.

Each neutral has a `-dark` counterpart in the frontmatter, applied under
`prefers-color-scheme: dark` unless the reader picked light, and under
`[data-theme='dark']` when they picked dark.

### Named Rules

**The One Accent Rule.** Enamel blue is the only hue on the site's own
surfaces. Colour beyond it arrives only inside product screenshots.

**The Reserved Amber Rule.** The stylesheet carries the app's amber
"Needs you" tokens so the two systems stay in step, but the site does not use
them. Amber means a person is needed on a card; it never decorates a page.

## Typography

**Body Font:** Figtree (with ui-sans-serif, system-ui, -apple-system, Segoe
UI, Helvetica, Arial)
**Label/Mono Font:** JetBrains Mono (with ui-monospace, SFMono-Regular,
Menlo, Consolas)

**Character:** One friendly, open grotesque does every job from the wordmark
to the footer, tightened with negative tracking as it grows; the mono is kept
for things a reader would copy or type.

### Hierarchy

- **Display** (700, 50px, 1, -1px tracking): the home-page wordmark only, set
  in em so the mark scales with it; 38px under 640px.
- **Lead** (600, 24px, 1.35): the one-sentence pitch under the wordmark, in
  Ink, centred, 640px max; 20px under 640px.
- **Headline** (700, 32px, 1.3): a docs page's `h1`; 26px under 640px.
- **Title Large** (700, 26px, 1.25): home-page band headings.
- **Title** (700, 22px, 1.3): docs `h2`, ruled above with a Border line and
  24px of padding; 20px under 640px.
- **Title Small** (700, 17px): card, stage and install-card headings; docs
  `h3` is 18px.
- **Body** (400, 16px, 1.65): running text, docs measure 780px. The home
  problem statement is 17px, left-aligned in a 640px column.
- **Body Small** (400, 15px): text inside cards and stages.
- **Control / Nav** (600 / 500, 14px): buttons, header links, sidebar links.
- **Label** (700, 11px, 0.08em, uppercase, Muted Text): the eyebrow above each
  home band, the sidebar group titles and the docs breadcrumb.
- **Mono** (400, 13px, 1.6): code blocks; inline code is 0.86em on a Panel
  chip; stage numbers are 12px 500.

### Named Rules

**The Label-Then-Heading Rule.** A home-page section opens with an uppercase
11px label naming its category, then its heading. The label orients; the
heading says something.

## Layout

- **Containers:** header, footer and docs at 1360px max; the home page at
  1120px; hand-written article pages at 820px; the docs article at 780px.
  Gutters are 24px, 16px under 640px.
- **Docs grid:** sidebar 248px, article, on-this-page 216px, 48px gaps. Under
  1200px the on-this-page column folds into an inline `details` block above
  the article and the sidebar narrows to 232px. Under 900px the sidebar
  becomes a slide-in drawer (300px or 86vw) over a scrim, opened from a menu
  button.
- **Home rhythm:** hero padded 64px top, bands 72px apart, cards and gallery
  items on 16px and 24–32px gaps. Feature grids run three columns, two under
  960px, one under 640px. Stage rows put the name in a 240px column beside the
  text, stacking under 960px.
- **Header:** sticky, 56px, Paper with a Border bottom edge; the scroll
  padding accounts for it.

## Elevation & Depth

The site is flat. No element carries a `box-shadow`. Depth is a matter of
tone and edge: Paper is the ground, Surface sits on it with a 1px Border, and
Raised and Panel tint regions inside a surface. Overlap happens only in the
mobile docs drawer, which sits over a scrim (`ink` at 45% in light,
near-black at 60% in dark) rather than casting a shadow.

### Named Rules

**The Flat Counter Rule.** Surfaces are separated by borders and tone, never
by shadow or blur. If something needs to stand out, give it the accent, not
height.

## Shapes

Gentle, consistent corners: 8px for controls, code blocks, tables and nav
items; 10px for cards, stage lists, screenshots and pager links; 5px for
inline code chips; full rounding for the stage badges. Every container has a
1px border; screenshots get the same border and 10px corner so they sit on
the page like cards. The focus outline is 2px Enamel Blue, offset 2px, with a
4px corner.

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
  sideways rather than wrapping.
- **Inline:** Panel chip, 5px corners, 0.86em, Ink.

### The stage list

The signature component: the five stages of the method as one bordered
Surface list, each row a numbered Enamel Wash badge (26px, mono) beside the
stage name and a one-line helper, then the stage's description. Divider lines
separate the rows. It mirrors the board's left-to-right columns as a top-down
reading order.

### Screenshot figure

Full-width product screenshots in light and dark pairs (`<picture>` switches
on `prefers-color-scheme`, and a script swaps the link target to the matching
full-size image), framed with the 1px Border and 10px corner, captioned in
13px Muted Text.

## Do's and Don'ts

### Do:

- **Do** take every colour from the custom properties in `site/src/site.css`,
  and keep them identical to the board app's tokens.
- **Do** separate surfaces with a 1px Border and a tonal step; keep everything
  flat.
- **Do** show the product with its own screenshots, always as a light and dark
  pair, framed like a card.
- **Do** open home-page sections with an 11px uppercase label before the
  heading.
- **Do** set keys, paths, commands and versions in JetBrains Mono.
- **Do** design both themes at once; dark is a warm near-black palette, not an
  inversion.
- **Do** keep any kitchen imagery to a sparing, restrained nod that earns its
  place; the toque mark is the anchor.

### Don't:

- **Don't** add shadows, glows or blur to lift a surface.
- **Don't** introduce a second accent hue, and don't use the amber "Needs you"
  colour for decoration.
- **Don't** turn the site into a dark hacker hero: no black-first terminal
  aesthetic, no neon green, no animated code.
- **Don't** use emoji or exclamation marks, in copy or as icons.
