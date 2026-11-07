# App skins

An app skin replaces the **layout** of Kinora's web app: the navigation, home,
library, detail and search pages. You write plain HTML templates and one CSS
file. There is no JavaScript: anything that would need code (links, playing,
search, scrolling rails, rotating heroes, TV remote navigation) is done by
Kinora when you add a `data-kn-*` attribute.

You don't override Kinora's styles to get there. A skin's markup is the page,
and its CSS only applies inside the skin, in a cascade layer that always beats
Kinora's own defaults. Write `.card { width: 300px }` and that's what you get.

Themes (colours, see [themes.md](themes.md)) and player skins (see
[player-skins.md](player-skins.md)) still work with an app skin: a skin reads
the theme's colours as CSS variables, and can bundle its own player.

- [Quick start](#quick-start)
- [Files](#files)
- [Templates and bindings](#templates-and-bindings)
- [What a page can read](#what-a-page-can-read)
- [CSS](#css)
- [Devices and TV remotes](#devices-and-tv-remotes)
- [Options](#options)
- [Installing, previewing and sharing](#installing-previewing-and-sharing)
- [Safety](#safety)

## Quick start

1. Turn on **Admin → Modules & themes → Custom app skins**.
2. In the new **App skins** card, press **Start from Marquee** (a full
   streaming look) or **Start from Blank** (the smallest skin that works).
   This copies the starter to `<config>/app-skins/<id>/`.
3. Press **Preview**. Only you see the skin, and while previewing, edits to the
   files show up within two seconds.
4. Edit the files. When you're happy, press **Use for everyone**.

From a terminal you can do the same without the web app:

```sh
kinora skin new my-skin --from marquee   # or --from blank; --dir to choose where
kinora skin check my-skin                # errors and warnings, like a compiler
kinora skin check my-skin --json         # the same, for editors and scripts
```

The starters also contain `kinora-skin.html-data.json` and `skin.schema.json`.
Copy `.vscode-settings.sample.json` to `.vscode/settings.json` and VS Code
autocompletes every `data-kn-*` attribute and every `skin.json` field.

If a skin ever gets in your way, open `#/?skin=off` (or switch on **Turn off
skins in this browser** in the card or in User Settings → Display). Kinora's
standard look comes back in that browser. `#/?skin=on` undoes it.

## Files

```
my-skin/
  skin.json            id, name, version, options (required)
  shell.html           the frame around every page (navigation, header)
  pages/home.html      one file per page; leave one out to keep Kinora's page
  pages/library.html
  pages/item.html
  pages/item.series.html   a variant: only for series
  pages/home.tv.html       a device version: only on TVs
  pages/search.html
  partials/card.html   reusable pieces, placed with data-kn-use="card"
  skin.css             all the styles
  assets/              images and fonts (png, jpg, webp, avif, svg, woff2)
  player/              optional: a player skin that comes with this skin
  preview.png          optional: shown in the admin card
  README.md            optional
```

Every page file is optional. A page without a template shows Kinora's own
page inside your shell, fenced off so your CSS doesn't touch it. Settings and
the admin pages always use Kinora's own pages.

The most specific template wins: `pages/item.series.tv.html`, then
`pages/item.series.html`, then `pages/item.tv.html`, then `pages/item.html`.
Item variants are `movie`, `series`, `season` and `episode`; devices are
`phone`, `tablet`, `desktop` and `tv`.

### skin.json

```json
{
  "id": "my-skin",
  "name": "My skin",
  "version": "1.0.0",
  "author": "You",
  "description": "One line for the admin card.",
  "skinApi": 1,
  "devices": ["phone", "tablet", "desktop", "tv"],
  "scheme": "dark",
  "theme": "marquee",
  "player": "./player",
  "options": {
    "card-size": { "type": "enum", "label": "Card size", "values": ["s", "m", "l"], "default": "m" }
  }
}
```

| Field | Meaning |
|---|---|
| `id` | Lowercase letters, digits and dashes. Must be unique on the server. |
| `skinApi` | The version of this guide the skin was written for. Kinora says in the admin card if it no longer supports it. |
| `devices` | Where the skin is used. Elsewhere, viewers get Kinora's standard look. Leave it out for every device. |
| `scheme` | `dark` or `light`: what your design assumes. |
| `theme` | A theme id the skin was designed with. The admin card offers to make it the default. |
| `player` | A built-in player style (`minimal`, `cinematic`, `floating`, `pro`), an installed player skin id, or `"./player"` for one in the skin's `player/` folder. Used when the admin's player style is "Follow the theme". |
| `options` | Settings the admin can change without editing files. See [Options](#options). |

## Templates and bindings

Templates are ordinary HTML. A small set of attributes connect them to Kinora:

```html
<section class="row" data-kn-each="rows">
  <h2 data-kn-text="title"></h2>
  <div class="rail" data-kn-rail>
    <a class="card" data-kn-each="items" data-kn-link="item">
      <img data-kn-img="poster" data-kn-size="320" alt="">
      <span data-kn-text="title"></span>
      <span class="badge" data-kn-if="isNew">New</span>
    </a>
  </div>
</section>
```

### Reading data

| Attribute | What it does |
|---|---|
| `data-kn-text="title"` | Sets the text. An empty value adds `data-empty` so CSS can hide it. |
| `data-kn-attr="title:overview aria-label:title"` | Sets attributes from values. |
| `data-kn-img="backdrop poster"` | On `<img>`: the first image kind the item has. Kinds: `poster`, `backdrop`, `thumb`, `logo`, `avatar`. Missing images get `data-missing`. |
| `data-kn-bg="backdrop"` | Sets `--kn-bg: url(…)` on the element, for CSS backgrounds. |
| `data-kn-size="480"` | The width to ask the server for (160, 240, 320, 480, 640, 780, 1280 or 1920). Images are resized and cached on the server. |
| `data-kn-progress="progress"` | Sets `--kn-progress` (0 to 1) and the width of a `data-kn-fill` child. |

Paths read the current scope: `title`, `nextUp.title`, `season.label`. The
whole app is always reachable as `app` (`app.user.name`, `app.libraries`).

### Structure

| Attribute | What it does |
|---|---|
| `data-kn-each="rows"` | Repeats the element per entry. Inside, paths read the entry. `@index`, `@number`, `@first`, `@last` and `@count` describe the loop. |
| `data-kn-empty="items"` | Shown only when the list is empty. |
| `data-kn-if="isNew"` | Rendered only when true. Also `!path`, `path=value` and `path!=value`. |
| `data-kn-switch="kind"` + `data-kn-case="series"` / `data-kn-default` | Renders the first matching child. |
| `data-kn-use="card"` | Fills the element with `partials/card.html`. |
| `data-kn-slot="outlet"` | In `shell.html`: where the page goes. |
| `data-kn-virtual` | On a long `data-kn-each`: renders 60 at a time as the viewer scrolls (big libraries). |

Repeated elements get state attributes for CSS: `data-kind`, `data-played`,
`data-in-progress`, `data-active`, `data-first` and `data-last`.

### Doing things

| Attribute | What it does |
|---|---|
| `data-kn-link="item"` | A link. `item`, `play`, `resume`, `parent`, `series` use the current item; `library` the current library; `season` the current season tab; `home`, `search`, `settings`, `admin`, `back` go where they say. The current page's link gets `aria-current="page"`. |
| `data-kn-action="play"` | A button: `play`, `resume`, `restart`, `toggle-played`, `logout`, `next-season`, `prev-season`. |
| `<input type="search" data-kn-search>` | Live search; typing opens the search page. |
| `<select data-kn-select="sort">` | Filled with the options and wired up: `sort` (library) or `season` (series). |
| `data-kn-component="user-menu"` | Kinora's own account menu (or `search-box`), in the theme's colours. |

Links and actions are the only way to navigate or change anything: `href`,
`src` and event handlers you write yourself are removed.

### Behaviours

| Attribute | What it does |
|---|---|
| `data-kn-rail` | A horizontal scroller with snap points. Its position is remembered when you come back. |
| `data-kn-scroll="prev"` / `"next"` | Buttons near a rail page it. They're disabled at the ends. |
| `data-kn-carousel="8s"` | Rotates its children, setting `data-current="true"` on the visible one. Pauses on hover, focus, a hidden tab and reduced motion. |
| `data-kn-carousel-dots` | Inside a carousel: filled with one button per slide. |

The skin root also gets `data-scrolled="true"` once the page has scrolled a
little (for headers that turn solid).

### Popovers and ids

Menus and dialogs use the browser's own popovers, no code needed:

```html
<button popovertarget="account">Account</button>
<div id="account" popover>
  <a data-kn-link="settings">Settings</a>
  <button data-kn-action="logout">Sign out</button>
</div>
```

Ids are prefixed with the skin id behind the scenes (so they never clash with
Kinora's), and `for`, `popovertarget`, `aria-*` references and `<use href>`
are rewritten to match. Write them as you normally would.

## What a page can read

Each page template starts with its own data at the top of the scope. The shell
starts with `App`.

| Page | Data |
|---|---|
| `shell.html` | `App` |
| `pages/home.html` | `Home` |
| `pages/library.html` | `Library` |
| `pages/item*.html` | `Item` |
| `pages/search.html` | `Search` |

**App**: `server.name`, `server.version`, `user` (`id`, `name`, `initial`,
`isAdmin`, `hasAvatar`), `libraries` (list of LibraryLink), `route` (`name`,
`path`), `search.query`, `options`, `device`, `input`.

**LibraryLink**: `id`, `name`, `kind` (`movies` or `tv`), `itemCount`, `active`.

**Card** (every title in a list): `id`, `kind`, `title`, `subtitle`,
`seriesTitle`, `episodeTitle`, `episodeCode` ("S1 · E2"), `year`, `rating`,
`officialRating`, `runtimeLabel`, `progress` (0 to 1), `remainingLabel`,
`played`, `inProgress`, `isNew`, `isPlayable`, `hasPoster`, `hasBackdrop`,
`hasThumb`, `hasLogo`, `overview`, `tagline`, `genres`, `genresLabel`,
`parentId`, `seriesId`.

**Home**: `rows` (each `id`, `title`, `kind` and `items`; `kind` is `continue`,
`next-up`, `latest` or `library`), `featured` (recent titles with a backdrop and
an overview, for a hero), `isEmpty`.

**Library**: `library` (a LibraryLink), `items`, `count`, `countLabel`,
`isEmpty`, `isLoading`, `sort` (each `value`, `label`, `active`).

**Item**: everything a Card has, plus `originalTitle`, `releaseDate`, `files`
(each `name`, `sizeLabel`, `container`, `resolution`, `videoLabel`,
`videoCodec`, `audioLabel`, `subtitleLabel`, `hdr`), `hasFiles`, `children`,
`seasons` (each `id`, `label`, `number`, `active`), `hasSeasons`, `season`,
`episodes` (the chosen season's), `hasEpisodes`, `nextUp`, `canPlay`,
`canResume`, `playLabel`, `resumeLabel`, `playedLabel`.

**Search**: `query`, `hasQuery`, `results`, `isEmpty`, `isSearching`,
`countLabel`.

`kinora skin check` warns about paths that don't exist, so typos show up
before anyone sees the page.

## CSS

Write `skin.css` as if it were the only stylesheet:

```css
/* Declarations at the top style the skin's root element. */
background: var(--kn-background);
color: var(--kn-foreground);

&[data-device="tv"] { font-size: 1.15rem; }

.card { flex: 0 0 300px; border-radius: 8px; overflow: hidden; }
.card:focus-visible { outline: 3px solid var(--kn-ring); }
.hero { background: var(--kn-bg) center / cover; }
```

- **It only applies inside the skin.** Kinora wraps it in
  `@scope ([data-app-skin="your-id"]) to ([data-kn-standard])`, so it can't
  reach Kinora's own pages, dialogs or the admin area.
- **It always wins.** It's in the last cascade layer, above Kinora's
  defaults, so you never need `!important` or long selectors.
- `html`, `body` and `:root` selectors mean the skin root.
- Theme colours are variables: `--kn-background`, `--kn-foreground`,
  `--kn-card`, `--kn-primary`, `--kn-primary-foreground`, `--kn-muted`,
  `--kn-muted-foreground`, `--kn-border`, `--kn-ring` and the rest listed in
  [themes.md](themes.md). Use them and the skin follows whatever theme is
  picked.
- `url(asset:hero.webp)` points at a file in `assets/`. Fonts work the same way
  in `@font-face`. Other URLs are removed.
- `@keyframes` names are made unique to the skin automatically.
- Option values are available as `--opt-<name>` and `[data-opt-<name>="…"]`.

### Layout building blocks

If you'd rather not write layout CSS, add `data-kn-layout` to an element and
tune it with variables. Your own CSS still wins over them.

| Value | Gives you | Variables |
|---|---|---|
| `stack` | A column with gaps | `--kn-gap` |
| `row` | A row with gaps, centred vertically | `--kn-gap` |
| `grid` | A responsive grid of cards | `--kn-card-w`, `--kn-gap` |
| `rail` | A horizontal row of cards (with `data-kn-rail`) | `--kn-card-w`, `--kn-gap` |
| `hero` | A tall area with its content at the bottom | |
| `split` | Two columns that stack on phones | `--kn-split` |
| `overlay` | Children layered on top of each other | |
| `center` | Content centred both ways | |

A tip from the starters: when a grid or flex container holds a rail, give its
column `minmax(0, 1fr)` (or the rail's row `min-width: 0`), or the rail
stretches the page instead of scrolling.

## Devices and TV remotes

The skin root carries `data-device` (`phone`, `tablet`, `desktop`, `tv`) and
`data-input` (`pointer`, `touch`, `remote`). Style for them in CSS, or write a
device version of a page (`pages/home.tv.html`).

To try another device in a desktop browser, open `#/?device=tv` (or `phone`,
`tablet`, `desktop`); `#/?device=auto` goes back to detecting it.

Arrow keys and TV remote D-pads move focus to the nearest link or button in
that direction, whatever your layout. Back (or Escape) goes back on TVs. You
can steer it:

| Attribute | What it does |
|---|---|
| `data-kn-focus-group` | Left and right stay inside the group (a row of cards). Coming back with up or down returns to the item focused there last. |
| `data-kn-focus-default` | Where focus starts on a page (the Play button, say). |
| `data-kn-focus-scroll="center"` | Focused items scroll to the middle instead of just into view. |

Make focus visible: `:focus-visible` is what a remote user sees.

## Options

Options let an admin adjust a skin from the admin card. Types:

| Type | Fields | Value |
|---|---|---|
| `bool` | `default` | `true` or `false` |
| `enum` | `values`, `default` | one of `values` |
| `number` | `min`, `max`, `default` | a number |
| `color` | `default` | any CSS colour, or empty for "use the theme's" |

Use them in CSS (`var(--opt-accent, var(--kn-primary))`,
`&[data-opt-card-size="l"] { … }`) or in templates (`data-kn-if="app.options.hero-rotate=true"`).
Marquee's carousel also stops when its `hero-rotate` option is `false`.

## Installing, previewing and sharing

- **Folder**: put the skin folder in `<config>/app-skins/` and press refresh in
  the admin card. Folders ending in `.sample` are ignored (the starters are
  kept there as `marquee.sample` and `blank.sample`).
- **Upload**: in the admin card, **Upload a folder** or **Upload a .zip**. The
  skin is checked first and only installed if it has no errors. Uploading a
  skin with an id that's already installed asks before replacing it.
- **Preview**: shows the skin only to you, in this browser tab, with live
  reload. Turn on live reload without previewing with `#/?skin-dev=1`.
- **Per viewer**: switch on **Let viewers pick their own skin** in the module
  settings, and everyone gets a picker in User Settings → Display.
- **Errors**: when a page fails in a viewer's browser, that viewer gets
  Kinora's page instead, and the error shows on the skin in the admin card.

Limits: 64 files, 512 KB of templates and CSS, 5 MB of assets.

## Safety

Skins are written by other people, so Kinora treats them as untrusted:

- Templates are checked against an allow-list of tags and attributes, on the
  server and again in the browser. Scripts, iframes, forms, event handlers,
  `style` URLs and links you write yourself are removed.
- CSS can only load the skin's own assets, and only applies inside the skin.
- Paths in templates only read the data listed above.
- Settings and admin pages always use Kinora's own UI, so a broken skin can't
  lock you out, and `#/?skin=off` always brings the standard look back.
