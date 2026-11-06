# Making a Kinora theme

A theme is a small JSON file. Kinora ships six (Kinora, Kinora Light, Midnight, OLED Black,
Ember, Daylight) and loads any extra ones from the `themes` folder inside your config
directory (`/config/themes` in Docker).

1. Copy `themes/example-ocean.json.sample` to a new file ending in `.json`,
   for example `themes/sunset.json`.
2. Change `id` (lowercase letters, numbers and dashes), `name`, and the colours.
3. Refresh the web app. Your theme shows up in the account menu, and admins can
   make it the default in Admin → Settings.

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

`scheme` is `"dark"` or `"light"` and tells the browser how to draw built-in
controls like scrollbars and the video player.

Any CSS colour works (`#ff7a59`, `rgb(...)`, `oklch(...)`). Values can't
contain `;`, `{`, `}`, `<` or `>`; invalid files are skipped and logged.

## Player style

A theme can also pick a video player layout and tweak its look. See
[player-skins.md](player-skins.md).

## Later

Themes use the same format that theme plugins and the marketplace will
distribute (see docs/plan.md §4), so a theme you make today keeps working.
