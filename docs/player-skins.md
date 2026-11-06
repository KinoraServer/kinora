# Player styles and custom skins

Kinora's video player has four built-in styles. They all have exactly the same
features (play, seek, volume, speed, audio, subtitles, fullscreen, picture in
picture, playback info, keyboard shortcuts). Only the layout and look differ.

| Style | What it looks like |
|---|---|
| Minimal | A slim bar along the bottom |
| Cinematic | Big centred play button, large title |
| Floating | A rounded glass bar floating over the video |
| Pro | Dense bar with labels, time remaining and "ends at" |

Viewers pick a style from the palette button in the player. The default is
"Theme default", which follows the active theme.

## Themes and the player

A theme can choose a layout and tweak the look with an optional `player` block:

```json
{
  "id": "sunset",
  "name": "Sunset",
  "tokens": { "primary": "#ff7a59" },
  "player": {
    "layout": "cinematic",
    "options": { "seek-height": "8px", "bar-blur": "20px" }
  }
}
```

`layout` is a built-in style id or the id of an installed skin. Options:

| Option | Meaning | Default |
|---|---|---|
| `bar-blur` | Blur behind the control bars | `16px` |
| `seek-height` | Thickness of the seek bar | `6px` |
| `button-radius` | Corner rounding of control buttons | `9999px` |
| `controls-opacity` | How solid the bars are, 0 to 1 | `0.85` |

A viewer's own choice always wins over the theme's layout. Every layout reads
the theme colours, so any theme restyles any style.

## Custom skins (your own markup and CSS)

A skin is a folder in `player-skins/` inside your config directory
(`/config/player-skins` in Docker):

```
player-skins/my-skin/
  skin.json    { "id": "my-skin", "name": "My skin", "author": "You" }
  skin.html    your markup
  skin.css     your styles (optional)
```

Copy `player-skins/example-neon.sample` to a new folder name that does not end
in `.sample`, then edit it. Refresh the web app and the skin appears in the
player's style menu (marked "custom"). Themes can select it with
`"layout": "my-skin"`.

### Wiring markup to the player

Add attributes to ordinary HTML elements; Kinora does the rest.

| Attribute | What it does |
|---|---|
| `data-kn-action="play"` | Play / pause. Others: `skip-back`, `skip-forward`, `mute`, `fullscreen`, `pip`, `info`, `back` |
| `data-kn-text="time"` | Fills the element with text. Others: `duration`, `remaining`, `title`, `method`, `resolution`, `buffer` |
| `data-kn-seek` | A seek bar. Children: `data-kn-fill` (played), `data-kn-buffer` (buffered), `data-kn-thumb` (handle, positioned with `left`) |
| `data-kn-volume` | A volume bar with a `data-kn-fill` child |
| `select data-kn-select="audio"` | Filled with the audio tracks. Others: `subtitles`, `speed`, `style` |
| `data-kn-interactive` | Lets any other element receive clicks |

The skin's root element carries state attributes you can style:
`data-playing`, `data-buffering`, `data-muted`, `data-fullscreen`,
`data-info` (each `true` or `false`) and `data-controls` (`visible` or
`hidden`, which follows the auto-hide).

### CSS

Your CSS is scoped to the skin automatically. Write declarations at the top
level to style the skin's root, and use `&[data-playing="true"] .big { ... }`
to react to state. Theme colours are available as `var(--kn-primary)`,
`var(--kn-card)` and so on. `@keyframes` works.

### Safety

Skins are checked twice (by the server when loading, and by the browser before
use). Scripts, event attributes such as `onclick`, frames, forms, `@import`,
`@font-face`, and anything that loads from another address (`url(https://...)`,
remote images) are removed. `data:image/...` URLs are allowed. So a skin can
change how the player looks and is laid out, but it cannot run code or send
requests, which keeps skins from other people reasonably safe to try.
A broken skin is skipped and logged instead of breaking playback.

Turn the whole feature off in Admin → Modules ("Custom player skins").
