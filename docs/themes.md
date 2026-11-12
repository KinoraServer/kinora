# Making a Kinora theme

A theme is a small JSON file. Kinora ships two (Kinora and Kinora Light). More install with
one click from the registry in Admin → Modules & themes (Midnight & Daylight, OLED Black,
Ember; see [modules.md](modules.md)), and Kinora loads your own from the `themes` folder
inside your config directory (`/config/themes` in Docker).

1. Copy `themes/example-ocean.json.sample` to a new file ending in `.json`,
   for example `themes/sunset.json`.
2. Change `id` (lowercase letters, numbers and dashes), `name`, and the colours.
3. Refresh the web app. Your theme shows up in Admin → Modules & themes, and
   admins can make it the default in Admin → Settings.

```json
{
  "id": "sunset",
  "name": "Sunset",
  "author": "You",
  "scheme": "dark",
  "tokens": {
    "background": "oklch(0.18 0.03 30)",
    "primary": "#ff7a59",
    "radius": "1rem"
  }
}
```

Tokens you leave out fall back to the Kinora theme.

## Light and dark pairs

To ship the same look in both schemes, make two files (one `"scheme": "dark"`,
one `"scheme": "light"`) and add `"pair": "<other id>"` to either of them:

```json
{ "id": "sunset", "scheme": "dark", "pair": "sunset-light", ... }
```

The theme picker then shows them as one card that follows the Light / Dark /
System switch. Kinora + Kinora Light is a built-in pair; Midnight + Daylight is one
from the registry.
A theme without a pair always uses its own scheme.

## Tokens

| Token | Used for |
|---|---|
| `background` / `foreground` | Page background and main text |
| `card` / `card-foreground` | Panels, dialogs, menus |
| `muted` / `muted-foreground` | Placeholders, secondary text, subtle fills |
| `primary` / `primary-foreground` | Buttons, links, progress bars, highlights |
| `accent` / `accent-foreground` | Hover states, secondary buttons, active nav item |
| `border` | Borders and dividers |
| `ring` | Keyboard focus outline |
| `destructive` | Delete buttons and errors |
| `radius` | Corner rounding, e.g. `0.75rem` |
| `font-sans` | Font stack, e.g. `Inter, system-ui, sans-serif` (the font must be installed on the viewer's device) |

The A–Z rail on library pages is drawn from these too: frosted `card` glass,
`foreground` letters, `muted-foreground` for letters with no titles and
`primary` / `primary-foreground` for the current letter.

`scheme` is `"dark"` or `"light"` and tells the browser how to draw built-in
controls like scrollbars and the video player.

Any CSS colour works (`#ff7a59`, `rgb(...)`, `oklch(...)`). Values can't
contain `;`, `{`, `}`, `<` or `>`; invalid files are skipped and logged.

## Player style

A theme can also pick a video player layout and tweak its look. See
[player-skins.md](player-skins.md).

## Changing the layout too

Themes only change colours and sizes. To change the layout of the whole app
(navigation, home, library, detail and search pages), write an app skin: see
[app-skins.md](app-skins.md). Skins read the theme's colours, so the two work
together.

## Later

Themes use the same format that theme plugins and the marketplace will
distribute (see docs/plan.md §4), so a theme you make today keeps working.
