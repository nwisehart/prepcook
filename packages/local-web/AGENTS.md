## Design System Styling Guidelines ("Clean station")

### Where the system lives

- Tokens: `../web-core/src/app/styles/new/index.css`. Light values sit on
  `:root`, dark overrides on `.dark` (the ThemeProvider puts `light` or `dark`
  on `<html>`). There is no scoping class: the tokens apply app-wide.
- Utilities: `tailwind.new.config.js` maps each token to a Tailwind color
  (`hsl(var(--x))`), so opacity modifiers work (`bg-brand/10`).
- Primitives: `../ui/src/components` (`Button`, `PrimaryButton`, `IconButton`,
  `ButtonGroup`, `Badge`, `Card`, `Input`, `Select`, `Dropdown`, `Dialog`,
  `KeyboardDialog`, `Switch`, `Tooltip`, `UserAvatar`, `PrepCookMark`, …).
  Reach for these before styling raw elements.

### Rules

- **The brand is blue `#3F6AA6`, never orange**, in both modes. Dark mode
  keeps the brand fill on buttons and steps brand-colored *text* up to
  `text-brand-secondary` for contrast.
- Use tokens only. No raw hex, `rgb()`, or Tailwind palette colors
  (`gray-*`, `zinc-*`, `neutral-*`, `blue-*`, `orange-*`, …) in TSX. If a
  color has no token, add one to `index.css` and the config instead of
  inlining it.
- shadcn-style names are **not** defined in this config and render nothing:
  `text-muted-foreground`, `bg-muted`, `bg-accent`, `bg-popover`, `bg-card`,
  `text-destructive`, `text-warning`, `ring-ring`, `border-input`. Use the
  tokens below.
- Dark is a selected palette, not an inversion: warm near-blacks, no pure
  black.
- Text never wears a data color, except a card's status line in its own
  semantic color. Counts next to a mini bar are muted text beside a brand bar.
- Copy: sentence case, no emoji, no exclamation marks. The attention state is
  **Needs you**, never "Needs Attention".

### Colors

Surfaces and lines:

| Utility | Light | Dark | Use |
|---|---|---|---|
| `bg-primary` (= `bg-background`) | `#F7F6F3` | `#1E1E21` | Page ground |
| `bg-secondary` | `#FFFFFF` | `#27282C` | Surface: cards, inputs, menus, dialogs, panels |
| `bg-panel` | `#EFEDE8` | `#2D2E33` | Hover and selected fill, neutral chip |
| `bg-surface-raised` | `#FBFAF8` | `#222225` | Archived / muted card |
| `border-border` (default border color) | `#E4E1DA` | `#37383E` | 1px borders |
| `border-divider` | `#F0EEE9` | `#33343A` | Hairline inside a card |

Text:

| Utility | Light | Dark | Use |
|---|---|---|---|
| `text-high` | `#1F2328` | `#ECEAE4` | Ink: titles, primary text, secondary-button labels |
| `text-normal` | `#4B4F56` | `#C9CBD0` | Body text (the default text color) |
| `text-low` | `#6A6F78` | `#A0A3AA` | Muted: helper lines, meta, placeholders, inactive icons |

Brand:

| Utility | Light | Dark | Use |
|---|---|---|---|
| `bg-brand`, `text-brand`, `ring-brand` | `#3F6AA6` | `#3F6AA6` | Primary buttons, active icons, working dots, focus ring |
| `bg-brand-hover` | `#2F5285` | `#4A7FBF` | Primary button hover |
| `text-brand-secondary` | `#2F5285` | `#8FB0DE` | Brand-colored text: links, "working" status lines |
| `bg-brand-soft` + `text-brand-soft-foreground` | `#E7EEF8` / `#2F5285` | `#243246` / `#9DBCE8` | Story chip, count chip, active tint, avatars, initial tiles |
| `text-on-brand` | `#FFFFFF` | `#FFFFFF` | Text and icons on a brand fill |

Needs you (amber):

| Utility | Light | Dark | Use |
|---|---|---|---|
| `bg-attention` | `#D29A1C` | `#E7B94C` | The dot |
| `text-attention-foreground` | `#9A6700` | `#E7B94C` | Amber text (status line, card strip, header chip) |
| `bg-attention-soft` | `#FFF4D6` | `#3A3120` | Needs-you strip and header chip |
| `border-attention-border` | `#EBD9A8` | `#5A4A1F` | Border of a Needs-you card |

Status:

| Utility | Light | Dark | Use |
|---|---|---|---|
| `text-success`, `bg-success` | `#1E7B4C` | `#5FC08A` | Merged, "Board up", additions (`+41`) |
| `bg-success-soft` | `#E3F3EA` | `#1F3A2C` | Success surface |
| `text-error`, `bg-error` | `#9B2C1F` | `#F0A39B` | Errors, bug chip text, deletions (`−7`) |
| `bg-error-soft` | `#FBE7E4` | `#432A28` | Bug chip, error alert surface |
| `text-merged` (+ `bg-merged/10`) | `#5A3FA6` | `#BBA7F0` | Epic chip |
| `bg-scrim/45` | ink | near-black | Dialog overlay |

CSS-only variables (no utility): `--warning` (the attention dot color),
`--console-background` / `--console-foreground` (terminal: `#1F2328` /
`#ECEAE4` light, `#141416` / `#ECEAE4` dark), `--console-success` /
`--console-error` (tuned for light surfaces in light mode), `--syntax-*`,
`--radius`.

Chip recipes: story `bg-brand-soft text-brand-soft-foreground`, bug
`bg-error-soft text-error`, epic `bg-merged/10 text-merged`, priority
`bg-panel text-normal`. `Badge` variants cover these (`default`,
`destructive`, `secondary`, `outline`).

### Typography

- **Figtree** (400/500/600/700) is the UI face: `font-sans`, already the
  default on `html`/`body`. **JetBrains Mono** (400/500): `font-mono`, for card
  keys (`PAN-49`), branch names, file paths, diff counts (`+41 −7`), versions
  and repo slugs. No emoji fonts are loaded.
- Root is 16px. Sizes: `text-xs` 12px (meta, chips, helper lines),
  `text-sm` 14px (body, card titles, buttons), `text-base` 16px, `text-lg`
  18px (panel, section and dialog titles), `text-xl` 20px. The page title is
  `text-[22px] font-bold tracking-[-0.3px]`.
- Weights: 700 for page/panel titles, column names and section labels; 600
  for card titles, buttons and amber status text; 500 for chips, nav items and
  "working" status text.
- Section labels: `text-[11px] font-bold uppercase tracking-[0.08em] text-low`.

### Shape, spacing, elevation

- Radius: `rounded` 8px (follows `--radius`: buttons, inputs, menus, nav
  items, segmented-control outer); `rounded-sm` 6px (segments, menu items);
  `rounded-md` 8px; `rounded-lg` 10px (cards, panels); `rounded-xl` 12px
  (dialogs); `rounded-full` (chips, dots, avatars, switches).
- Cards carry **no shadow**. `shadow-overlay` is for menus, popovers,
  tooltips and dialogs; `shadow-panel` for the card panel floating over the
  board.
- Spacing: Tailwind's scale plus `half` 4px, `base` 8px, `plusfifty` 12px,
  `double` 16px. Gutters are 12px between columns and cards (`gap-3`); page
  padding 28px with 22px on top; card padding 14px × 12px (`px-3.5 py-3`).
  Column width 236px, sidebar 232px, card panel 520px.
- Buttons are 32px tall (`h-8`; 36px `h-9` in page headers), `px-3`, 600.
  Inputs are 36px. Chips are 22px pills, `px-2`, 12px 500. Dots are 8px
  (`size-2`), 10px in column headers.
- Icons: Phosphor, regular weight, 16px in nav and buttons (`size-4`), 14px
  inline in meta (`size-3.5`). The `size-icon-*` tokens are also available
  (`icon-xs` 15px, `icon-sm` 17.5px, `icon-base` 20px). Never emoji.

### Focus states

Focus rings are 1px brand and inset:
`focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand`.
The base layer makes every `:focus` ring inset, and the default ring color is
the brand.

### Patterns

- **Status line on a card**: one line, 8px dot + text.
  - Needs you: `bg-attention` dot, `text-attention-foreground font-semibold`
    ("Waiting for your approval").
  - Working: `bg-brand` dot, `text-brand-secondary font-medium`
    ("Planning · 9 min", "Writing the fix · 12 min", "Reviewing · fix round 2").
  - Idle: 14px muted icon, `text-low` ("Imported 1 h ago").
  - Merged: `bg-success` dot, `text-success font-semibold` ("Merged").
- **Needs-you card**: the card takes `border-attention-border overflow-hidden`
  and an amber strip at the top (`bg-attention-soft text-attention-foreground
  text-xs font-bold px-3.5 py-2` with a dot) reading "Needs you · the RCA is
  ready" or "Needs you · merged, ready to close". The body holds the actions:
  at the Plan gate **Approve** (primary) + **Review** (secondary, opens the
  session/RCA); at Close a **Close card** (secondary) button.
- **Header "2 need you" chip**: an amber pill in the page header
  (`h-6 rounded-full bg-attention-soft text-attention-foreground font-semibold`
  with a dot); clicking it filters the board to those cards.
- **Column header**: a 10px dot (hollow grey for To Do, `bg-brand` for
  Plan…Push MR, `bg-success` for Close, grey for Clean up) + name in 700 +
  muted count, with the helper line in `text-xs text-low` beneath.
- **Card**: `bg-secondary border rounded-lg px-3.5 py-3 flex flex-col gap-2`.
  Top row: key in `font-mono text-xs text-low` + chips; title
  `text-sm font-semibold text-high`; status line; optional footer row above a
  `border-t border-divider`. Archived (Clean up) cards use
  `bg-surface-raised` with the title in `text-normal`.
- **Sidebar nav** (232px): brand row (`PrepCookMark` in `text-brand` +
  "Prepcook" 17px 700), project switcher (bordered surface button with a
  `bg-brand-soft` initial tile), nav items (icon + label, `rounded`, active
  `bg-panel` with the icon in brand), a spacer, the **mode card** (surface,
  bordered: "Chef mode" + `Switch`, a one-line description, a "Switch to Prep
  mode" link), and a footer with the `font-mono` version and a green
  "Board up" dot.
- **Segmented control** (Active / All / Mine): `ButtonGroup` +
  `ButtonGroupItem`; active segment `bg-panel text-high font-semibold`.
- **Dialogs**: `bg-secondary border rounded-xl shadow-overlay` over
  `bg-scrim/45`; title `text-lg font-bold text-high`.

### Example component styling

```tsx
// Primary and secondary buttons (or use <Button> / <Button variant="outline">)
className="h-8 px-3 rounded bg-brand text-on-brand text-sm font-semibold hover:bg-brand-hover"
className="h-8 px-3 rounded border bg-secondary text-high text-sm font-semibold hover:bg-panel"

// Input
className="h-9 px-3 rounded border bg-secondary text-sm text-high placeholder:text-low focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand"

// Chip
className="inline-flex h-[22px] items-center gap-1.5 rounded-full px-2 text-xs font-medium bg-brand-soft text-brand-soft-foreground"

// Needs-you status line
<span className="flex items-center gap-2 text-xs font-semibold text-attention-foreground">
  <span className="size-2 rounded-full bg-attention" />
  {t('…')}
</span>
```

### Architecture Rules

- **View components** (in `views/`) should be stateless - receive all data via props
- **Container components** (in `containers/`) manage state and pass to views
- **UI components** (in `ui-new/`) are reusable primitives
- File names in `ui-new/` must be **PascalCase** (e.g., `Field.tsx`, `Label.tsx`)
