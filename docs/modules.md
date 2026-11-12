# Modules and the registry

Kinora ships small. Optional features are **packages** you install from a registry in
**Admin → Modules & themes → Add modules, themes and skins**, and uninstall the same way.
Nothing in that list is part of the server until you press Install.

- [What can be installed](#what-can-be-installed)
- [What stays built in](#what-stays-built-in)
- [Installing, updating and uninstalling](#installing-updating-and-uninstalling)
- [How a package is checked](#how-a-package-is-checked)
- [What a module can and can't do](#what-a-module-can-and-cant-do)
- [Other clients](#other-clients)
- [Offline servers and your own registry](#offline-servers-and-your-own-registry)
- [Upgrading from a version with built-in modules](#upgrading-from-a-version-with-built-in-modules)
- [API](#api)

## What can be installed

| Kind        | What it is                                          | Where it lands               |
| ----------- | --------------------------------------------------- | ---------------------------- |
| Module      | Server logic as one sandboxed WebAssembly file      | `<config>/modules/<id>/`     |
| Theme       | One or more theme files (design tokens)             | `<config>/themes/<id>.json`  |
| Player skin | Markup and CSS for the video player                 | `<config>/player-skins/<id>/`|
| App skin    | Templates, CSS and assets for the whole app         | `<config>/app-skins/<id>/`   |

The official registry ([KinoraServer/kinora-registry](https://github.com/KinoraServer/kinora-registry))
carries:

- **TMDB metadata** (module): titles, descriptions and artwork. Needs a free API key.
- **Auto audio & subtitles** (module): see [auto-tracks.md](auto-tracks.md).
- **Activity log** (module): fills Admin → Activity.
- **Midnight & Daylight**, **OLED Black**, **Ember** (themes).
- **Neon** (player skin) and **Marquee** (app skin).

A module provides one or more extension points:

| Extension point     | The server asks it to                                             |
| ------------------- | ----------------------------------------------------------------- |
| `metadata_provider` | look up a movie, a series or an episode during a scan              |
| `track_selection`   | remember a viewer's audio/subtitle choice and pick tracks at start |
| `event_subscriber`  | react to server events (playback, scans, new users)                |

## What stays built in

Three engines are part of the server, because they render inside the web app and can't be
swapped at runtime. Each can be switched off, and costs nothing while it is off:

- **Custom themes**: shows installed themes and your own files. Kinora's own two themes
  (Kinora and Kinora Light) are always there.
- **Custom player skins** and the four built-in player styles.
- **Custom app skins**, plus the tiny Blank starter that `kinora skin new` copies.

The same goes for the places a module plugs into: the player sends track changes, Admin →
Activity shows the log, the scanner asks for metadata. Without a module behind them they do
nothing. A module can't add pages or scripts to the web app; its settings form is drawn by
Kinora from the fields the module declares.

## Installing, updating and uninstalling

- **Install** downloads the newest version this server can run. A module is switched on as
  soon as it is installed; its card with its settings appears further down the page.
- **Update to x.y.z** appears when the registry has a newer version. Settings and saved data
  are kept. Kinora never updates a package on its own.
- **Uninstall** (the bin) removes the files. A module's settings and saved data stay unless you
  tick "Also delete its settings and saved data", so installing it again picks up where it left
  off. An app skin that was in use for everyone is switched back to the standard look.
- The list is fetched from the registry at most every 10 minutes; the refresh button asks again.

Installed themes and skins are overwritten by an update. To change one, copy it under a new id
and edit the copy. Kinora never overwrites a theme or skin folder you put there yourself: if
your own file has the same name, the install is refused.

## How a package is checked

A registry is a folder of static files:

```text
index.json                              every package and its versions
index.json.sig                          Ed25519 signature of index.json
packages/<id>/<version>/package.json    the manifest: its files and their SHA-256
packages/<id>/<version>/files/...
```

1. The index's signature is checked against the keys this server trusts (when it has any, see
   below).
2. The manifest is downloaded and must match the SHA-256 the index lists for it.
3. Every file is downloaded into a staging folder and must match the size and SHA-256 in the
   manifest. File paths must be plain relative paths; a package is at most 300 files and 20 MB.
4. The files get the same checks as ones you add by hand: themes and skins go through the
   theme, player skin and app skin validators; a module must be valid WebAssembly that imports
   nothing but Kinora's one host function.
5. Only then are the files moved into place. A failure at any step leaves nothing installed.

## What a module can and can't do

A module is WebAssembly run by an interpreter inside the server (module ABI 1). Every call gets a
fresh instance, at most 64 MB of memory and a limit on how long it may compute. It has no file
system, no network and no clock of its own. It reaches the outside through host functions, and
only the ones its manifest asks for, which the list shows before you install:

| Permission          | Lets the module                                                       |
| ------------------- | --------------------------------------------------------------------- |
| `http: [hosts]`     | send HTTPS requests to exactly those hosts (no redirects elsewhere)    |
| `storage`           | keep its own key/value data in Kinora's database, per viewer or shared |
| `activity`          | add lines to Admin → Activity                                          |
| (none needed)       | write to the server log                                                |

A module can't read your media files, other modules' data, users' passwords or sessions. A module
that crashes or runs too long fails that one call; the server keeps going.

Writing a module is covered in the registry repository (`docs/modules.md` there).

## Other clients

Modules run on the server, so what they do reaches every client: the Android TV app gets
TMDB artwork and the auto-picked audio and subtitle tracks without any change. Themes, player
skins and app skins are web-app looks; the Android TV app doesn't use them. Native apps can't
load code at runtime, so anything that needs new screens in a client ships with that client.

## Offline servers and your own registry

Everything already installed keeps working without internet; only the list needs the registry.

| Variable              | What                                                                          |
| --------------------- | ----------------------------------------------------------------------------- |
| `KINORA_REGISTRY_URL` | The folder that holds `index.json`. Default: the official registry.            |
| `KINORA_REGISTRY_KEY` | Base64 Ed25519 public key(s), comma separated, the index must be signed with.  |

A registry is plain files, so a copy on any web server (or a mirror on your LAN) works. With
`KINORA_REGISTRY_URL` set and no key, the index isn't checked for a signature; files are still
checked against the index.

## Upgrading from a version with built-in modules

Older versions had TMDB, Auto audio & subtitles, the Activity log and the Midnight, Daylight,
OLED Black and Ember themes compiled in. The first time an existing server starts on this
version it installs those from the registry in the background, so nothing goes missing
(modules you had switched off are skipped). Settings such as the TMDB key and every viewer's
remembered tracks carry over. If the registry can't be reached, it tries again on the next start;
you can also install them by hand. A new server starts with nothing installed.

Marquee was a starter you copied, so your copy in `app-skins/` is untouched. The registry's
Marquee installs as `marquee`; if you already have a folder with that name, rename yours first.

## API

All admin only.

| Call                                    | What                                                         |
| --------------------------------------- | ------------------------------------------------------------ |
| `GET /api/registry[?refresh=true]`      | the registry's packages merged with what is installed         |
| `POST /api/registry/{id}/install`       | install, or update to the newest version                      |
| `DELETE /api/registry/{id}[?purge=true]`| uninstall; `purge` also deletes a module's settings and data  |
| `GET /api/modules`                      | built-in engines and installed modules, with their settings   |
| `PUT /api/modules/{id}`                 | switch one on or off, change its settings                     |
