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

- Registry list (Admin > Modules & themes): one bordered list of packages with
  filter chips by kind; each row shows what a module may do ("It can: ...")
  before the Install button, so permissions are read before installing.
  Installed rows swap Install for an Update button (only when there is one)
  and a ghost bin; uninstalling always asks in a dialog. Built-in engines carry
  a "Built in" badge and have no bin. (`web/src/pages/admin/Registry.tsx`)
- Admin list rows: row actions are small ghost icon buttons (pencil to edit,
  bin to remove) after any labelled action, each with an `aria-label`. Adding
  and editing share one dialog; fields that can't change after creation stay
  visible but disabled, with a help line saying why.
  (`web/src/pages/admin/Libraries.tsx`)
- A setting lives on the page for what it acts on, not on General: the
  automatic-scan interval ("Scan schedule" card) moved from Settings to the
  Libraries page, because it only concerns libraries. It still saves through
  `PUT /api/system/settings` (the endpoint takes the whole object, so the
  card echoes the loaded values back). (`web/src/pages/admin/Libraries.tsx`)

## Player UI

- Player chrome sits over the video on `bg-black/80`–`bg-black/85` with white
  text. This is a deliberate exception to the token rule: the backdrop is the
  video, not the theme. (`web/src/components/MediaPlayer.tsx`)

## Open questions

- (record questions you couldn't decide alone here; delete the entry once
  settled)
