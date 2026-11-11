# Design decisions

The log of deliberate design choices in Kinora's web UI. Newest entries first.
Each entry is one line: the decision, the why, and where it lives in code.
When a decision changes, fix the entry in place — this file must always read
as the current truth, never as a history.

## Color and theming

- All colors are read from `--kn-*` tokens (`web/src/index.css`), mapped into
  Tailwind via `@theme inline`. Why: runtime themes (docs/themes.md) restyle
  the app without touching components.
- The default "Kinora" theme is a green-tinted near-black (background
  `#0b1612`, primary `#2bd48f`). It should read as calm and cinematic, not as
  a developer dashboard.
- Hover states: `hover:brightness-*` on filled buttons, `hover:bg-accent` on
  ghost/text. No separate hover colors
  (`web/src/components/ui/button.tsx`).
- Focus: `focus-visible:ring-2 focus-visible:ring-ring` on every interactive
  element — TV remotes are keyboards, so the ring is the primary cursor.

## Elevation

- Shadows only for floating layers (dialogs, menus, overlays),
  `shadow-xl`/`shadow-2xl`. Cards and tiles are flat. (`ui/dialog.tsx`,
  `AppLayout.tsx`)

## Layout and typography

- (empty — record decisions here as they're made)

## Components and interaction

- Side sheets (the watch party panel) slide in from the right, `max-w-sm`, full
  width on phones, on `bg-card` with `shadow-2xl`, and stay inside the player's
  frame so they work in fullscreen. (`web/src/watch-party/WatchPartyPanel.tsx`)

## Player UI

- Player chrome sits over the video on `bg-black/80`–`bg-black/85` with white
  text. This is a deliberate exception to the token rule: the backdrop is the
  video, not the theme. (`web/src/components/MediaPlayer.tsx`)

## Open questions

- (record questions you couldn't decide alone here; delete the entry once
  settled)
