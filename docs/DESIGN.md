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
  Content pages (Overview, Health, Cleanup, Duplicates, Media,
  Applications, Games, Developer Tools, Docker, Downloads, History)
  use an 880 px column; forms and chat (Settings, Assistant) use a
  narrower 760 px reading column.
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
glyph) + "DiskPilot" at 13 px semibold. Top-level items: Overview,
Health. Groups: **Analyze** — Storage, Media, Duplicates, Downloads,
Assistant (only when enabled in Settings); **System** — Applications,
Games, Developer Tools, Docker; **Clean up** — Cleanup (with a
trailing count of actionable recommendations), History. The footer,
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
  hatched inside its category segment. Segments are separated by a
  1 px inset line in the panel colour, and their widths grow in on
  mount (500 ms; collapsed under reduced motion). Hovering a segment
  shows the floating-card tooltip — never a `title` attribute — with
  the category, its size and its share of the disk, plus the likely
  recoverable part when there is one; the tooltip layer sits outside
  the clipped bar so the card is never cut off. Legend items never
  wrap mid-item.
- **Figures** — one figure style everywhere: 12 px muted label, 20 px
  semibold tabular value. Overview stats, the since-last-scan deltas
  and the Docker summary cards all use it.
- **Size bar** — the single meter implementation for the whole app:
  a proportional bar under a figure, in the row's category colour
  (accent when the row has no category). 2 px under figures, 4 px in
  the Overview category list. The premium treatment lives in this one
  component (see Charts): at 4 px and taller the fill is a gradient
  from the full series colour to the same colour lightened (82 %
  colour into white), over a track of the border token at half
  strength; at 2 px the fill stays flat, where a gradient could not
  be seen anyway. Widths grow in on mount and transition on change,
  always with fully rounded caps.
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
- **Assistant messages** — user messages keep the accent bubble
  (right-aligned, 86 % max width). Assistant replies sit flat on the
  page in the modern chat pattern: the app logo glyph in a 24 px
  raised circle beside a full-width content column — no bubble;
  code blocks and cards provide their own surfaces. A muted 11 px
  timestamp (HH:MM) sits under each message; assistant messages
  reveal a Copy action (raw markdown source) on hover or focus.
  Replies are rendered markdown, never raw characters: paragraphs at
  body size with 8 px block spacing; headings restrained (15/14/13 px
  semibold — a chat column, not a document); compact lists with muted
  markers; inline code as a raised mono chip; fenced code as a
  `panel-raised` panel (10 px radius, hairline border, mono 12 px,
  horizontal scroll) with a Copy button; blockquotes take a left
  accent hairline and muted text; tables are full-width at 12.5 px
  with hairline row dividers and a muted semibold header, scrolling
  horizontally when needed; GFM task-list checkboxes are disabled and
  take the accent token. Links never navigate the webview — absolute
  URLs open through the external opener. Long words and URLs always
  wrap; the column never scrolls horizontally. Highlighted
  recommendations render inline under the reply in a raised
  container titled "Worth a look": one compact row per item (tier
  badge, title, size), each row and the "Review in Cleanup" footer
  action entering the same Cleanup review flow. While awaiting a
  reply, a thinking row shows the assistant mark with three dots
  pulsing in sequence (static under reduced motion). A failed request
  renders as a danger-tinted callout (callout spec, critical tint)
  with a Retry action that resends the last message without
  duplicating it.
- **Page toolbar** — mode switches and page-level actions (the
  Duplicates view switch and Review button, the Storage tabs) sit
  on a toolbar row under the page header, never inside it — so a
  header description always takes the full column width and wraps
  in balanced lines, never around the controls.
- **Overview composition** — one hierarchy, in order: the Display
  headline with the page's actions on its row; the meta line
  (drive · total · scan age) on its own full-width line beneath —
  it never wraps, the drive name truncates instead; the storage
  strip and its legend; the figures row; Since last scan (with
  Biggest changes as its panelled sub-section); Forecast; Drives;
  Start here; Where space goes; Largest consumers; notices;
  Scan details. New content joins this order — it never inserts
  itself above the strip.
- **Drives** — a quiet section after Forecast: one hairline row per
  drive (drive icon, label, a 4 px Size bar in accent, "{free} free
  of {total}" in tabular figures, and a trailing action column).
  The scan root's row is labelled "Scan root"; every other drive
  gets a subtle "Scan" button. No panels — the rows are a footnote
  to the figures above. The section hides itself when the only
  drive is the scan root's.
- **Forecast card** — the Since-last-scan callout spec, looking
  forward: a label row, a 15 px semibold headline ("fills in about
  N days", or the flat/shrinking wording when there is no fill-up
  date), a 12 px muted explanation with the daily rate and
  projected date — then the shared Chart in its mini configuration
  spanning the card's full width beneath the text: the smooth
  accent curve over its gradient area and a single dashed
  baseline, with the hover crosshair and tooltip on (see Charts).
- **Biggest changes** — a callout panel inside the Since-last-scan
  section, titled "Biggest changes", holding two lists with muted
  sub-headers, "Grew most" and "Freed": name over a `mono-path`
  trimmed path, signed delta right-aligned — review-tier colour
  for growth, safe-tier green for freed space. A list renders only
  when it has rows.
- **Cloud card** — a notice-row (callout spec) with a cloud icon: how
  much the provider keeps on this PC, its file count and folder, and
  a "Free up space" action. The confirmation dialog states the
  contract in plain words — files stay in the cloud, re-download when
  opened, nothing is deleted — before the single primary action.
- **Treemap (Storage Map)** — the current folder level as
  proportional blocks inside a framed surface (hairline border,
  10 px radius, page background showing through the gaps). A level
  earns blocks by size: entries show until 140 blocks or 99.5 % of
  the level's bytes are covered; the rest fold into one aggregate.
  Blocks are filled with their category colour mixed into the
  panel colour at the `--map-tint` strength (22 % light, 33 %
  dark); neighbouring blocks of the same category alternate a
  ±4-point tint step so individual files read as distinct tiles.
  No per-block borders — 2 px gaps and a 4 px radius do the
  separation. Labels: name at 12 px medium with the size at 11 px
  muted beneath, only when the block fits them (≥ 64 px wide,
  ≥ 30 px / 46 px tall), otherwise the tooltip carries both. The
  aggregate is a waffle — a grid of small neutral cells, one per
  folded item up to 400, with its label on a translucent panel
  chip — so a long tail reads as many small files, never an empty
  slab; it is inert. Folders open on click; files select into the
  inspector. Selection is an inset accent ring. Navigation is the
  explorer's breadcrumb: an up-level ghost button, quiet ancestor
  buttons, and the current folder as a raised chip.
- **Preview inspector** — the explorer's details panel: facts first
  (size, type, category), then a Media section for images (dimensions,
  camera, capture date from EXIF, each shown only when the file
  carries it). The footer stacks Reveal in Explorer with, for files
  under the cloud root, "Free up space" (same cloud dialog).
- **Quick Search panel** — the frameless popup is one floating
  surface (raised fill, `border-strong` outline, deep soft shadow,
  10 px radius) at 680 × 520: a 12 px drag strip, a 44 px search field
  at 15 px with an `esc` hint chip, the same "Understood as" chips as
  Storage, and a keyboard-first results list (selected row takes the
  `selected` background; the footer states the match count and the
  key hints). Empty states follow the standard spec, scaled to the
  panel.
- **Settings rows** — one control column for the whole page:
  right-hand controls share a right edge; selects are 160 px
  (schedule, theme, provider alike); toggles sit at the same right
  edge; full-width fields (scan root, exclusions, API key) span the
  column. Labels on the left, descriptions under them in muted
  12 px — a row never stacks its control under its label.
- **Watch settings** — a Settings section in the standard row form:
  the schedule select, the two alert thresholds as number fields in
  a fixed structure (96 px input, unit in a fixed 132 px span —
  "GB", "GB between checks" — so both inputs share left and right
  edges), and the sign-in toggle reflecting the registration the
  shell reports, not the last click.
- **Guided cleanup items** — recommendations another tool owns
  (Windows.old) render with no checkbox, a muted "Guided" tag beside
  the tier badge (a plain ring-bordered chip, never a tier colour),
  and their guidance as an info callout inside the expanded row.
- **HTML report** — the exported Overview report is a standalone
  light document: wordmark header (accent "Pilot"), figure row,
  hairline tables, a raised forecast panel, and a print stylesheet.
  It never loads external assets.
- **Health page** — the disk's state as three sections. The header
  subtitle is the verdict, computed from the worst drive ("All
  drives look healthy" / "One drive needs attention"). **Drives**
  extends the Overview's drive rows with a status chip — the tier
  tag treatment, never a new style: free ≥ 20 % is "Healthy"
  (safe tier), 10–20 % "Getting low" (review tier), under 10 %
  "Low space" (critical). **Forecast** is the Overview forecast at
  page scale — Fills in / Daily growth / Projected full as figures
  (or the flat/shrinking wording) — over the growth chart: the
  shared Chart at ~140 px, scanned size as the accent area + curve,
  free space as a muted second line on the same scale, the min–max
  ranges as a caption beneath (the Chart's own axis carries the
  ticks and end dates). Under two history points the section is
  the instructive empty state instead. **Watch** is a definition-list panel (the
  Scan details grid inside a hairline panel) restating the
  schedule, both alert thresholds, sign-in state and last scan,
  with an "Open Settings" action — Health reports the watch; it
  never edits it.
- **Media page** — the scan's photos and videos. A figures row
  (Photos, Videos, Screenshots — each bytes over its file count;
  Screenshots is captioned "included in Photos", because it is a
  subset, never a separate total). Biggest videos and biggest
  photos are hairline lists: name over a `mono-path` parent folder
  and date caption, size right-aligned; a row click reveals the
  file in Explorer. An empty list is a quiet "None found in this
  scan" line, not an empty state. When look-alike groups exist, a
  callout strip (the OneDrive card's pattern) states the group
  count and the extra copies' size and offers "Review in
  Duplicates" — which opens the Duplicates page on its look-alike
  view, because the mode lives in the store. Before the first scan
  the page is the standard empty state.
- **Games page** — installed Steam and Epic games, read from the
  launchers' manifests (no scan involved). A figures row (Total in
  games, Games, Libraries), then one section per library: the
  source as title, the library path as the description, the
  library subtotal as the section action; game rows are name over
  a 2 px accent Size bar (against the library's largest game),
  size right-aligned, and a Reveal ghost button. A closing notice
  states the boundary plainly: uninstalls happen in Steam or
  Epic — DiskPilot never removes game files. No libraries found
  is the standard medallion empty state.
- **Welcome (first run)** — a full-window surface that replaces
  the shell (sidebar hidden, drag region kept) exactly when setup
  was never finished or skipped and no scan exists; it waits for
  startup to settle and never appears when settings failed to
  load. One scrolling 640 px column: the logo mark, a Display
  welcome line, three value rows (36 px raised icon tile,
  semibold title, muted line), then the setup — the scan-root
  row and the Watch rows in their Settings form. Every choice is
  a draft: the primary "Start first scan" persists them all,
  with `onboarded`, through the settings save flow and starts
  the scan; "Skip setup" persists `onboarded` alone. It never
  replays.
- **History restore** — a cleanup entry with items the shell can
  still restore carries a Restore action in its detail area;
  permanent-only and zero-restorable entries carry nothing. The
  confirm is inline — the question ("Restore N items to their
  original locations?") replaces the button, with Restore and
  Cancel — and the outcome stays in the entry as a callout-spec
  banner: the restored count, the first failure when any item
  failed, and the permanently deleted count when items were
  skipped. The list refreshes behind it; the banner survives the
  refresh.
- **Text match chip** — a search hit that matched only through
  text read inside the file carries the neutral marker tag
  "Text match" beside its name (tooltip: "Matched text inside
  this file"). A plain marker, never a tier tag. Used in Storage
  search results and Quick Search. The reading itself is the
  Settings → Scanning toggle "Read text in images", on by
  default; reading is on-device (Windows built-in OCR) and the
  text is never sent anywhere.

## Charts

One chart system, bespoke SVG in this design language — no chart
library. Geometry lives in `src/lib/chart.ts` (pure, unit-tested);
rendering in `src/components/Chart.tsx`. Charts appear only where
the data is a series: the Health growth chart and the Overview
forecast. Nothing else in the app draws a line.

- **Curve** — a Catmull-Rom spline through the points, converted to
  cubic Bézier segments, stroked at 1.5 px with round caps and
  joins. Control points are clamped into the plot box, so the curve
  approaches the data's extremes but never overshoots them.
- **Area** — under the primary (first) series only: a vertical
  gradient from the series colour at 24 % opacity to transparent.
  Later series are lines on the same scale, never filled.
- **Axes and grid** — the y scale is bracketed by nice ticks (steps
  of 1, 2 or 5 × 10ⁿ, about four); each tick is a horizontal
  hairline in the border token, dashed 2 4 at 60 % opacity, with
  its label in 11 px muted tabular figures in a reserved left
  gutter. The first and last x values are captioned at the bottom
  corners beneath the plot.
- **End point** — the primary series' last point carries a filled
  dot with a 2 px ring in the surface colour and a soft halo
  (the series colour at 18 %), so the eye lands on "now".
- **Crosshair and tooltip** — hovering snaps to the nearest point:
  a vertical hairline in `border-strong`, a dot on every series,
  and the floating-card tooltip (raised fill, `border-strong`
  outline, deep soft shadow) with the date and each series' value
  beside its colour dot. The card flips to the other side of the
  crosshair near the right edge, and above/below the point near
  the top. It leaves cleanly when the pointer does.
- **Keyboard** — the chart is focusable; ← and → step the
  crosshair point by point, and the chart's accessible label
  always describes the active point ("date — label value, …").
- **Mini configuration** — the sparkline form (Overview forecast):
  no y-axis labels, the grid collapses to the single baseline at
  the scale's foot, the hover crosshair and tooltip stay on.
- **States** — a single point is a dot with a caption, never a
  fake line; no points render nothing, and the caller's empty
  state speaks instead.
- **Motion** — on first mount the line draws in and the area fades
  over ~600 ms ease-out. Under `prefers-reduced-motion` the global
  rule collapses both to instant; nothing in a chart ever loops
  or pulses.
- **Bars** — meters share the premium treatment through the one
  Size bar component (gradient fill at 4 px and taller, flat at
  2 px, border-token track at half strength, rounded caps, width
  transitions), and the storage strip follows the same language:
  panel-coloured 1 px separators, grow-in widths, and the
  floating-card tooltip per segment — see the Storage strip and
  Size bar entries under Components.

## Motion

| Transition        | Duration |
| ----------------- | -------- |
| Hover states      | 120 ms   |
| Expanding rows    | 180 ms   |
| Sheets / dialogs  | 180 ms   |
| Bar / strip grow  | 300–500 ms |
| Chart draw-in     | 600 ms   |

All durations collapse to zero under `prefers-reduced-motion`.

## Iconography

Lucide throughout: 16 px in the sidebar, 14 px inline, stroke 1.75.
The app icon and tray icon are the product's own marks — the app icon
is an indigo rounded square with a white gauge glyph; the tray icon is
a ring rendered in code that fills as the disk fills (indigo arc over a
neutral track).
