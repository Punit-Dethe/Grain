# Grain Space — design tokens

A small, deliberately minimal system. Not a full framework — just the shared
scales every Grain Space surface should pull from so spacing, type, and shape
stay consistent. Tokens live as CSS custom properties on `.gs-frame`
(`grain-space.css`); use `var(--gs-…)` rather than raw px.

## Spacing — 4px base

| token         | px |
| ------------- | -- |
| `--gs-sp-1`   | 4  |
| `--gs-sp-2`   | 8  |
| `--gs-sp-3`   | 12 |
| `--gs-sp-4`   | 16 |
| `--gs-sp-5`   | 20 |
| `--gs-sp-6`   | 24 |
| `--gs-sp-8`   | 32 |

Rhythm: rows use `sp-2` vertical padding, panes use `sp-4`–`sp-5` insets,
section gaps use `sp-5`–`sp-6`.

## Type scale

| token          | px   | use                          |
| -------------- | ---- | ---------------------------- |
| `--gs-fs-11`   | 11   | meta, timestamps, counts     |
| `--gs-fs-12`   | 12   | small labels, chips          |
| `--gs-fs-13`   | 13   | body, list rows, editor      |
| `--gs-fs-14`   | 14   | section headings, emphasis   |
| `--gs-fs-16`   | 16   | card titles                  |
| `--gs-fs-26`   | 26   | note title                   |

Weights: `--gs-w-normal` 460 · `--gs-w-medium` 520 · `--gs-w-semi` 600 ·
`--gs-w-bold` 700. Prefer weight + color over size to signal hierarchy.

## Radius

| token         | px | use                    |
| ------------- | -- | ---------------------- |
| `--gs-r-sm`   | 8  | rows, small controls   |
| `--gs-r-md`   | 10 | buttons, chips         |
| `--gs-r-lg`   | 14 | inputs, cards          |
| `--gs-r-xl`   | 18 | panes, sheet           |

## Color intent

Warm paper neutrals carry the UI; **orange is an accent only** — focus rings,
active markers, the avatar, the caret. Never large orange fills (they read
cheap). Depth comes from hairline borders + very soft shadows, not heavy
contrast. See the palette tokens at the top of `grain-space.css`.
