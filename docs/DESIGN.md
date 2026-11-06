# DiskPilot design system

DiskPilot's interface is a native-feeling Windows desktop UI: quiet,
dense where a professional tool should be dense, and always leading
with the answer. This document is the single source of truth for
tokens, components and principles. Tokens live in
`src/styles/globals.css`; components consume tokens, never raw values.

## Principles

1. **Answer first.** The Overview opens with the state of the disk, not
   with controls. Every page puts its conclusion above its evidence.
2. **Risk is always visible.** Cleanup tiers are coloured and labelled
   everywhere they appear; nothing destructive hides behind neutral
   styling.
3. **One accent, one job.** Indigo means "interactive / selected /
   primary". It is never decoration.
4. **Lines before boxes.** Hairline separators structure pages; panels
   are flat. No shadows on content — elevation is reserved for
   floating surfaces (menus, dialogs, tooltips), which step up to
   `panel-raised` with a `border-strong` outline and a deep soft
   shadow.
5. **Motion answers actions.** Animation explains what just happened
   (expand, sheet, hover), never decorates. All motion respects
   `prefers-reduced-motion`.
6. **Skeletons mirror layout.** Loading states have the same shape as
   the content they replace.
7. **Empty states instruct.** An empty state says what will appear
   there and offers the one action that fills it.
8. **No emoji in the UI.** Icons are Lucide, 16 px in navigation and
   14 px inline, stroke width 1.75.

## Colour

### Light theme

| Token           | Value     | Use                                   |
| --------------- | --------- | ------------------------------------- |
| background      | `#FFFFFF` | Window background                     |
| sidebar         | `#F5F5F7` | Sidebar background                    |
| panel           | `#FFFFFF` | Content panels                        |
| panel-raised    | `#F5F5F7` | Raised/inset surfaces, floating UI    |
| hover           | `#EDEDF0` | Row/control hover                     |
| selected        | `#DEDEE6` | Selected row / active nav             |
| border          | `#E5E5EA` | Hairlines                             |
| border-strong   | `#D1D1D6` | Emphasised borders, inputs            |
| foreground      | `#1D1D1F` | Primary text                          |
| muted           | `#6E6E73` | Secondary text                        |
| subtle          | `#737378` | Captions, metadata                    |
| accent          | `#5856D6` | Interactive, selected icon, primary   |
| tier-safe       | `#248A3D` | Likely safe                           |
| tier-review     | `#B25000` | Review first                          |
| tier-keep       | `#6E6E73` | Keep                                  |
| critical        | `#D70015` | Destructive, errors                   |

### Dark theme

| Token           | Value     | Use                                   |
| --------------- | --------- | ------------------------------------- |
| background      | `#1C1C1E` | Window background                     |
| sidebar         | `#161618` | Sidebar background                    |
| panel           | `#1C1C1E` | Content panels                        |
| panel-raised    | `#252528` | Raised/inset surfaces                 |
| hover           | `#2A2A2D` | Row/control hover                     |
| selected        | `#323236` | Selected row / active nav             |
| border          | `#2E2E31` | Hairlines                             |
| border-strong   | `#3A3A3D` | Emphasised borders, inputs            |
| foreground      | `#F5F5F7` | Primary text                          |
| muted           | `#A1A1A6` | Secondary text                        |
| subtle          | `#8E8E93` | Captions, metadata                    |
| accent          | `#7D7AFF` | Interactive text/icons                |
| accent-fill     | `#5E5CE6` | Primary button fill                   |
| tier-safe       | `#30D158` | Likely safe                           |
| tier-review     | `#FF9F0A` | Review first                          |
| tier-keep       | `#98989D` | Keep                                  |
| critical        | `#FF453A` | Destructive, errors                   |

### Category colours

Categories are always a coloured dot + plain text, never a filled
pill.

| Category         | Light     | Dark      |
| ---------------- | --------- | --------- |
| Application data | `#2A78D6` | `#3B8EF0` |
| Build artifacts  | `#E0601F` | `#E0692A` |
| Docker storage   | `#13906F` | `#18A88A` |
| Cache            | `#B07F00` | `#B8870C` |
| User data        | `#D24B84` | `#D9578F` |
| Downloads        | `#2D8A3A` | `#3FA34D` |
| Developer tools  | `#6A52D9` | `#8B6CF0` |
| Logs             | `#D33D3D` | `#E05555` |
| Binary data      | `#8A7F73` | `#9A8F84` |
| System files     | `#6B7280` | `#7C8494` |
| Unknown          | `#A1A1AA` | `#5B5F68` |

### Tier tags

The tier tag is the only coloured tag in the product: an icon + label
in the tier colour on a faint tint of the same colour. Categories never
get this treatment, so a tier tag always reads as "risk level".

## Typography

System font stack — Segoe UI on Windows, `-apple-system` fallback —
with a monospace stack (Cascadia Code, `ui-monospace`, Consolas) for
paths and commands. All sizes use tabular figures.

| Style      | Size / line | Weight |
| ---------- | ----------- | ------ |
| Display    | 28 / 32     | 600    |
| Figure     | 20 / 24     | 600    |
| Page title | 15 / 20     | 600    |
| Section    | 13 / 18     | 600    |
| Body       | 13 / 18     | 400    |
| Secondary  | 12 / 16     | 400    |
| Caption    | 11 / 14     | 400    |

Sentence case everywhere; no all-caps labels. Sizes in tables are
right-aligned and tabular, with an optional 2 px meter under the value.

## Layout

- 4 px spacing grid: 4, 8, 12, 16, 24, 32. Page gutters are 24 px.
- Window: 1280 × 820 default, minimum 980 × 640.
- **One page shell for every page.** Pages scroll inside a shell with
  24 px gutters and 24 px top padding, and the content column is
  left-aligned so every page title starts on the same left edge.
  Content pages (Overview, Cleanup, Duplicates, Applications,
  Developer Tools, Docker, History) use an 880 px column; forms and
  chat (Settings, Assistant) use a narrower 760 px reading column.
  Storage is the one full-bleed page, because its tables need the
  width. No page centres its column — a shared left edge is what
  makes ten pages read as one system.
- Sections are separated by 28 px, with a 13 px semibold title, an
  optional 12 px description, and a 10 px gap before the content.
- Sidebar: 220 px, `sidebar` background, right hairline, 38 px drag
  region at the top.
- Radii: 4 px tags, 6 px buttons/inputs/rows, 10 px panels, sheets and
  dialogs.
- Table rows 32 px; two-line rows 44 px.

### Navigation

Sidebar header: logo mark (indigo rounded square with a white gauge
glyph) + "DiskPilot" at 13 px semibold. Groups: Overview; **Analyze** —
Storage, Duplicates, Applications, Developer Tools, Docker, Downloads,
Assistant (only when enabled in Settings); **Clean up** — Cleanup
(with a trailing count of actionable recommendations), History. The footer,
above a hairline, shows scan status (not scanned yet / scanning with an
indeterminate bar and the current path in 11 px mono / scanned
<relative time> with the root path) and the Settings nav button.

Nav buttons are 28 px tall, `rounded-md`, 13 px text; the active item
uses the `selected` background, medium-weight text and an accent icon,
inactive icons are `muted`, hover uses the `hover` background. Group
labels sit 20 px below the previous group. The footer scan status is
separated from the nav by a hairline and 12 px of air.

## Components

- **Buttons** — primary (accent fill), secondary (outline), ghost, and
  destructive (critical; only inside confirmations). Heights 24 / 32 /
  36 px (small / default / large), icon buttons 28 px. The default
  height matches the field height (32 px), so a button beside an
  input always lines up. Filled buttons carry a faint top highlight;
  a disabled primary or destructive button does not fade its colour —
  it becomes a neutral `selected`-grey pill with `subtle` text, so a
  disabled action reads as the same button, switched off.
- **Fields** — inputs, selects and textareas are 32 px, `border-strong`
  outline with a soft inset shadow; the border strengthens on hover
  and turns accent on focus (in addition to the global focus-visible
  outline). Checkboxes and radios take their colour from the
  `accent-fill` token, never a hard-coded hex.
- **Search fields** — one pattern everywhere: a 16 px search icon
  centred in a relatively positioned wrapper, the input padded 32 px
  on the left. Icons are never positioned with magic offsets.
- **Storage strip** — the whole disk as one horizontal bar: category
  segments, remaining used space, free space; recoverable space is
  hatched inside its category segment. Legend items never wrap
  mid-item.
- **Figures** — one figure style everywhere: 12 px muted label, 20 px
  semibold tabular value. Overview stats, the since-last-scan deltas
  and the Docker summary cards all use it.
- **Size bar** — the single meter implementation for the whole app:
  a proportional bar under a figure, in the row's category colour
  (accent when the row has no category). 2 px under figures, 4 px in
  the Overview category list.
- **Callouts** — informational panels (notices, warnings, the
  since-last-scan summary) share one spec: 10 px radius, hairline
  border, `panel-raised` fill, 16 px horizontal and 12 px vertical
  padding.
- **Floating surfaces** — dialogs, menus and tooltips sit one step
  above the page: `panel-raised` fill, `border-strong` outline and a
  deep, soft shadow. Dialogs open over a 50 % black backdrop with a
  2 px blur, so the whole window — sidebar included — recedes.
  Dialog padding rhythm: 20 px sides; header 20 px top / 12 px
  bottom; body 12 px; footer 14 px with a hairline above it.
- **Choice cards** — selectable option cards (the cleanup mode
  radios) show their state: the checked card takes an accent border
  and a 5 % accent tint; unchecked cards strengthen their border on
  hover.
- **Empty states** — a 48 px medallion (icon at 24 px inside a
  `panel-raised` circle with a hairline ring), a 13 px semibold
  title, a description no wider than 420 px, and the action 20 px
  below. Pages give the empty state room — a minimum height or the
  remaining flex space — so it centres in the space it fills instead
  of hugging the header.
- **Classification card** — category, rule ID, explanation, confidence,
  and inherited-from, shown in the Storage details panel.
- **Tier badge / category label** — as defined above; never
  interchangeable.
- **Details panel** — live metadata + classification card + actions
  (Reveal in Explorer) for the current selection.
- **Cleanup dialog** — the plan sheet: exact paths, warnings, rejected
  items, action choice (Recycle Bin default; permanent only when
  offered), and a single Confirm.
- **Skeletons** — mirror the layout of the page they stand in for.
- **Tooltips** — figures and icon buttons carry short explanations
  (e.g. every Overview figure label has an Info tooltip).

## Motion

| Transition        | Duration |
| ----------------- | -------- |
| Hover states      | 120 ms   |
| Expanding rows    | 180 ms   |
| Sheets / dialogs  | 180 ms   |

All durations collapse to zero under `prefers-reduced-motion`.

## Iconography

Lucide throughout: 16 px in the sidebar, 14 px inline, stroke 1.75.
The app icon and tray icon are the product's own marks — the app icon
is an indigo rounded square with a white gauge glyph; the tray icon is
a ring rendered in code that fills as the disk fills (indigo arc over a
neutral track).
