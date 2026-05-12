# IDC Design System

Extracted from [option-2-reception.html](./option-2-reception.html) and [option-2-accounting.html](./option-2-accounting.html). This is the visual language all screens inherit. No mention of features or flows — purely what things look like and why.

---

## 1. Color

### 1.1 Surface tokens

The whole app sits on a near-white canvas with a whisper of warmth. There are only three surface tones, and they layer in a strict order.

| Token | Hex | Where it lives |
|-|-|-|
| `--paper` | `#FDFDFA` | The canvas. Main background, sidebar, header, status bar — the entire chrome. |
| `--paper-2` | `#F7F5EE` | Inline accents only: table headers, row hover, sunken filter-pill containers, role pills, count badges. Never used for whole regions. |
| `--surface` | `#FFFFFF` | Cards and panels. The only pure-white surface. Pops cleanly against the warm paper. |

The unified chrome is intentional. Sidebar matches main; status bar matches main. The only thing that visually rises is `--surface` (white cards) and `--ink` (dark hero cards). No nested cream-on-cream layering.

### 1.2 Lines

| Token | Hex | Use |
|-|-|-|
| `--line` | `#ECE8DB` | Default border, table dividers, panel separators |
| `--line-2` | `#DDD8C7` | Heavier borders: inputs, ghost buttons, focused/hovered elements |

### 1.3 Ink (text + dark surface)

| Token | Hex | Use |
|-|-|-|
| `--ink` | `#0A1230` | Primary text, dark "ink card" surfaces (reversed scheme), brand mark fill |
| `--ink-2` | `#1C2851` | Body text in dense areas, secondary buttons |
| `--ink-3` | `#5E5A4E` | Meta text, labels, table column heads (warm-leaning gray, not cold) |
| `--ink-4` | `#94907F` | Disabled, placeholders, decorative dots, weekend bars |

Ink tones are warm-biased gray — they read as desaturated navy/stone, not neutral gray. This keeps the whole palette cohesive with the cream paper.

### 1.4 Brand & semantic

| Token | Hex | Role |
|-|-|-|
| `--crimson` | `#C0263A` | Brand accent. Primary actions (`Save`, `New visit`, `Close day`). Eyebrow rules. Danger states. The hero color that earns attention. |
| `--crimson-dark` | `#9A1F30` | Hover/pressed for crimson buttons. |
| `--crimson-soft` | `#FBE9EC` | Background tint for crimson chips and aging alerts. |
| `--gold` | `#B45309` | Warning / pending / partial state. Accountant role. Outstanding 60-90d aging. |
| `--gold-soft` | `#FEF3E2` | Background tint for warnings. |
| `--success` | `#047857` | Paid, done, closed, on-target. |
| `--success-soft` | `#ECFDF5` | Background tint for done chips. |
| `--info` | `#1D4ED8` | Receptionist role. Arrived / in-waiting state. Aging 30-60d bucket. |
| `--info-soft` | `#EFF4FD` | Background tint for info chips. |

### 1.5 Role color coding

The same token-set powers all roles, but each role's primary avatar and `.role-pill` dot picks a different semantic color:

- **Receptionist** → `--info` (#1D4ED8) — calm, operational
- **Accountant** → `--gold` (#B45309) — financial, premium
- **Superadmin** → `--crimson` (#C0263A) — authority

A user always sees their own role color reflected in their avatar tile and the role pill at the top of the sidebar.

---

## 2. Typography

### 2.1 Family

- **Inter** for everything. No serif. (Earlier drafts used Fraunces — dropped for professionalism.)
- **Geist Mono** for tabular numbers: money, counts, time, IDs (`P-2841`), version strings.

### 2.2 Tracking

Body: `letter-spacing: -0.011em`. Display: tighter, `-0.012em` to `-0.026em` depending on size. Tracking is *negative* at every size — Inter is designed for it.

### 2.3 Scale

| Use | Size | Weight | Notes |
|-|-|-|-|
| Page title | 30px | 700 | `-0.026em`, line-height 1.1 |
| KPI value | 30-32px | 700 | tabular numerals, mono unit suffix smaller |
| Panel title | 14-15px | 600 | `-0.01em` |
| Body | 13-14px | 400-500 | default reading size |
| Status / chip | 11px | 600 | uppercase, `0.04em` tracking |
| Eyebrow / column head | 10-11px | 600 | uppercase, `0.1-0.12em` tracking |
| Meta / sub | 11-12px | 400-500 | `--ink-3` |
| Mono ID / time | 10-12px | 500-600 | Geist Mono, `font-feature-settings: 'tnum'` |

Sizes are restrained on purpose. Hierarchy comes from weight + tracking + color, not from giant font sizes. The biggest type on a page is rarely larger than 32px.

### 2.4 Numerals

Anything that's a *number* — money, counts, time, IDs, percentages — gets Geist Mono with `font-feature-settings: 'tnum'` so digits stack vertically. This is non-negotiable on tables, KPIs, and ledger rows.

---

## 3. Geometry

### 3.1 Radii

| Token | Value | Use |
|-|-|-|
| `--radius` | `6px` | Buttons, inputs, filter pills, small chips |
| `--radius-lg` | `12px` | Cards, panels, KPI tiles |
| (specific) | `8px` | Sidebar nav items, icon buttons, avatar tiles |
| (specific) | `4px` | Status tags, table-internal accents |
| `999px` | pill | Role pills, status pills, count badges, dots, avatars (circular) |

Sharper corners on small UI, softer on larger surfaces. Pills are reserved for status — never used as buttons.

### 3.2 Spacing

The shell is fixed: `256px` sidebar / fluid main / `64px` header / `32px` status bar.

Main content padding: `28px 36px 60px` (top / sides / bottom — extra bottom is intentional).

Inside panels:
- Panel head: `14px 20px`
- Panel body row: `12-18px 20-24px`
- Card padding: `18-24px`

Gutters between panels: `16-20px`. Between sections: `24px`.

### 3.3 Shadows

Shadows are rare. Borders carry separation 90% of the time. Shadows appear only on:

- Hover lift on cards: `translateY(-1px)` + `0 4-6px 12-16px rgba(10, 18, 48, 0.04)`
- Active filter pill ("the chosen tab in a sunken container"): `0 1px 2px rgba(10, 18, 48, 0.06)` to lift it above its tray
- Crimson button hover: `0 6px 16px rgba(192, 38, 58, 0.15)` — a warm glow, never gray drop-shadow

No card uses a base-state shadow. Borders + tone do the work.

---

## 4. Motion

### 4.1 Easing & timing

```css
--ease: cubic-bezier(.2, .7, .2, 1);
```

Used on every transition. It's quick-out, slow-in — feels confident, not bouncy.

| Speed | Use |
|-|-|
| 140ms | Hover-state color swaps, filter pills, nav highlights |
| 160-180ms | Standard hover, button color/background |
| 200-240ms | Card lifts, larger surface fades, toggle slides |
| 1.5-1.8s | Status dot pulse/blink (`animation: blink` infinite) |

### 4.2 Hover convention

Every interactive surface changes at least two things on hover (color + background, or color + lift). Single-property hovers feel cheap. Buttons add `translateY(-1px)` for primary, never for ghost.

### 4.3 Live indicators

State dots that imply *now* (a patient in the chair, sync running, an arrival mid-stream) get the `blink` animation. Static dots (waiting, scheduled, done) don't. This means the eye can scan a queue and instantly see what's live.

---

## 5. Components

### 5.1 The dark ink card

A panel can flip its scheme: `background: var(--ink)`, text `var(--paper)`, dividers `rgba(253, 252, 248, 0.12)`. Used sparingly for the financial focal point (e.g. "today's net" on Daily Close). One per page maximum. The brightest visual moment.

### 5.2 Status pill

```
[dot] LABEL TEXT
```

- 11px Inter 600, uppercase, `letter-spacing: 0.04em`
- Padding `4px 10px`, radius `999px`
- A `::before` dot 6px is mandatory — color matches the label
- Background is the soft variant (`--success-soft`, `--gold-soft`, etc.)
- Live states pulse the dot

### 5.3 Role pill

Anchored at the top of the sidebar under the brand. Shows the active role: `[dot] ACCOUNTING`. Token-styled (`--paper-2` bg, `--line` border), 10px Inter 600 uppercase, `0.06em` tracking. The dot color encodes the role.

### 5.4 Count badge

The little number on a nav item (e.g. `Patients · 347`, `Outstanding · 14`). Mono font, `1px 7px` padding, `999px` radius. Tints:

- Default: `--paper-2` bg / `--ink-3` text
- Active nav (inside a dark `--ink` item): `rgba(255,255,255,0.18)` bg / `--paper` text
- Alert: `--crimson` bg / white text (e.g. unpaid count)
- Warn: `--gold` bg / white text (e.g. day needs closing)

### 5.5 KPI tile

Lives inside a `kpi-strip` — a one-pixel-gap grid that looks like a zebra plate.

```
LABEL · uppercase 10.5px
30M               ← Inter 700, 30-32px, tnum
↑ 14% vs Apr      ← trend, success or crimson
```

No shadow at rest. Hover swaps background to `--paper`. Currency unit (`د.ع`) renders smaller and in `--ink-3`.

### 5.6 Filter pills (segmented)

A row of pills inside a `--paper-2` tray with `--line` border. Active pill flips to `--surface` (white) with a hair-line shadow. Used for `All / Waiting / Done / Unpaid` style choices and period selectors (`Today / Week / Month`).

### 5.7 Eyebrow rule

The little tag above page titles: a 18-20px crimson hairline `::before` followed by uppercase meta text.

```
─── WEDNESDAY · 12 MAY 2026 · 10:24
Good morning, Asma.
```

Signals the editorial heritage of the design without using italic serifs anywhere.

### 5.8 Quick-action grid

`2 × 2` cards with: icon (18px) top-left, bold label, muted sub-line. Hover lifts and lights to white. Used in side rails to expose primary-but-secondary actions.

### 5.9 Aging bar

In the receivables aging panel, each bucket gets a `6px × 36px` vertical bar in its semantic color:

- Current: `--success`
- 30-60d: `--info`
- 60-90d: `--gold`
- 90+d: `--crimson`

The bars line up to form a tiny aging spectrum at a glance.

### 5.10 Day-status tag

Used in the recent-days table:

- `Closed` → success
- `Open` / `Open · needs close` → gold (and tinted row in the close UI)
- `Locked` → ink-4 ghost (read-only, archived)

Compact, mono-adjacent, 10.5px 600 uppercase.

---

## 6. Iconography

- Lucide-style line icons (or matching custom), `stroke-width: 1.8-2`, `stroke-linecap: round`, `stroke-linejoin: round`.
- Standard sizes: `14px`, `16px`, `18px`. `13px` only in tight buttons.
- `currentColor` always — icons inherit text color, never hardcoded.
- Inside nav items, icons sit on the left with `11px` gap to label.

---

## 7. Tables

- Header row: `--paper-2` bg, `10px` uppercase Inter 600 with `0.1em` tracking, `--ink-3` color, `12px 20-22px` padding.
- Body row: `13px` body text, mono for numeric columns, `12-14px 20-22px` padding.
- Hover: row background `--paper`, cursor pointer.
- Footer / totals row: `--paper-2` bg, weights bumped to 600, mono for totals.
- Right-align all numeric columns. The header label aligns right too.

---

## 8. Inputs

- Background `--paper-2` at rest, `--surface` on hover and focus.
- Border `--line-2` (slightly heavier than panel borders for affordance).
- Focus: border becomes `--ink`, plus `box-shadow: 0 0 0 3px rgba(10, 18, 48, 0.08)`.
- Padding `12px 14px` for full inputs, `9px 12px` for compact.
- Radius `--radius` (6px) — never pill.

Labels above inputs are 11px Inter 600 uppercase with `0.08em` tracking — same as the eyebrow voice.

---

## 9. Buttons

| Variant | Background | Text | Border | When |
|-|-|-|-|-|
| `btn-primary` | `--crimson` | white | `--crimson` | The single most important action on the screen. One per primary view. |
| `btn-ink` | `--ink` | `--paper` | `--ink` | Save / commit when crimson is reserved for something more dangerous on the same screen. |
| `btn-ghost` | transparent | `--ink-2` | `--line-2` | Secondary actions (Export, Discard, Print). The workhorse. |
| `btn-danger` | white | `--crimson` | `--crimson` (1px) | Destructive but reversible (soft-delete, reset). Inverted so it doesn't shout. |
| `btn-sm` | (modifier) | | | Tightens padding to `6-7px 10-12px` and font to `12px`. |

Hover on primary lifts `1px` and dyes the surface darker. Ghost hovers swap background to `--paper-2`. Never use dropshadow on ghost.

---

## 10. The chrome (shell)

The frame is intentionally quiet:

- **Sidebar**: same `--paper` as the canvas, separated only by a right border. Brand mark + role pill + grouped nav. User card pinned to bottom with a lock icon.
- **Header**: `64px`, just a breadcrumb on the left, icon buttons + language pill + avatar on the right. No title — the title lives in the page content.
- **Status bar**: `32px`, mono-weighted, holds the sync pill and the version/device identifier. Always present.

Three things never change between screens: the brand mark position, the avatar position, the sync pill in the status bar. Every other element can shift.

---

## 11. Voice (visual)

- **Restrained.** Big sizes are earned, not assumed. The default mood is calm.
- **Tabular.** Numbers are first-class — always mono, always tnum, always right-aligned in tables.
- **Warm but not soft.** The palette is warm; the typography is precise. The contrast is the point.
- **Status before content.** Every list item leads with state (pill, badge, color). Reading the screen at a glance should answer "what state is this in?" before "what is this?".
- **One hero per screen.** One crimson button. One dark ink card. One page title. Spread out the heat.

---

## 12. RTL

The whole system is direction-agnostic. Specifically:

- Right-aligned numeric columns flip left in RTL.
- Eyebrow rule flips to the right side of the text.
- Toggle thumbs translate negatively.
- Active-nav side-indicator (the 3px left bar) mirrors to the right edge.
- Status pill dots stay leading their label regardless of direction.

Arabic-Indic numerals (٠١٢٣) are a setting (`arabic_numerals: bool`) that affects rendering of all tabular numbers when enabled. Geist Mono falls back gracefully when the digit shape switches.

---

## 13. Token quick-reference

```css
:root {
  /* Surfaces */
  --paper: #FDFDFA;
  --paper-2: #F7F5EE;
  --surface: #FFFFFF;
  --line: #ECE8DB;
  --line-2: #DDD8C7;

  /* Ink */
  --ink: #0A1230;
  --ink-2: #1C2851;
  --ink-3: #5E5A4E;
  --ink-4: #94907F;

  /* Brand & semantic */
  --crimson: #C0263A;
  --crimson-dark: #9A1F30;
  --crimson-soft: #FBE9EC;
  --gold: #B45309;
  --gold-soft: #FEF3E2;
  --success: #047857;
  --success-soft: #ECFDF5;
  --info: #1D4ED8;
  --info-soft: #EFF4FD;

  /* Geometry */
  --radius: 6px;
  --radius-lg: 12px;

  /* Motion */
  --ease: cubic-bezier(.2, .7, .2, 1);

  /* Type */
  --display: 'Inter', system-ui, sans-serif;
}
```

That's the whole system.
