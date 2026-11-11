# Watch parties

Built-in module `watch-party` (Admin → Modules → "Watch parties"). It is **off by default**.
Turned on, it lets people watch a movie or an episode together, in sync, using
[Open Watch Party](https://openwatch.party) (OWP), an open protocol for synced viewing.

Each person plays their **own copy** of the title from their own Kinora server. Nothing is
streamed between people: a small relay only keeps everyone's players in step (play, pause,
seeking, speed). So a friend who joins needs the same title in a library they can play.

## Turning it on

1. Admin → Modules → Watch parties → switch it on.
2. That's all. Parties use the public Open Watch Party relay, which needs no account or key.

### Public relay or your own

The public relay is a free, shared, public service. While a party runs it sees each person's
display name, the app name (`Kinora/<version>`), the title's details (title, year, season and
episode, catalogue IDs such as TMDB, the file size) and the playback position. It deletes them
when the party ends. Anyone who has a party's code can join it. See the relay's
[privacy notice](https://openwatch.party/privacy) and [terms](https://openwatch.party/terms).

**For fully private watch parties, run your own relay** and enter its base URL in the module's
"Your own relay" setting, e.g. `https://relay.example.com` (or `http://192.168.1.5:8080` on your
own network). How to run one: <https://openwatch.party/relay#self-hosting>. It is a single Go
binary or a Docker image (`ghcr.io/edde746/owp-relay`) that gets its own certificate.

Parties on your own relay and parties on the public relay can't see each other: everyone in a
party must use the same relay. Leave the setting empty to go back to the public relay.

## Using it

- **Start a party**: the popcorn button in the top bar (or in the player) → "Start a party". You
  are the host. Share the code it shows (for example `K7M-Q2X`).
- **Share a title**: as the host, play something. What you play is shared with the party, and
  playing something else switches everyone over.
- **Join**: popcorn button → type the code → Join. Kinora looks for the party's title in your
  libraries and opens the player on it.
- Pressing play, pause or seeking in the player goes to the whole party. By default only the
  host controls playback; the host can let everyone control it.

### How your copy is found

1. Movies and episodes that share a catalogue ID (TMDB, IMDb, TVDB, ...) with the party's title,
   whatever they are called. Kinora stores TMDB IDs when the TMDB module has matched the title.
2. Only if no ID matched: the title search. Movies by title and a year at most one apart,
   episodes by the show's title with the same season and episode numbers.
3. The copy whose length is within 1.5 seconds of the host's is picked. If none is that close, you
   are asked whether to play the closest one anyway (it may be a different cut) or sit it out.

The panel always shows **your copy** next to the party's title, so you can spot a wrong episode.
"Wrong episode or version? Pick another file" lists the other candidates and lets you search your
library for a title filed under another name.

### Offset

For a different cut (a longer intro, an extended edition), shift your copy against the party in
steps of 0.1 or 1 second. Your position is the party's position plus the offset, so a longer
intro needs a positive offset. The offset resets when the title changes.

### Host tools

Who controls playback (only the host, or everyone), lock the party to new joins, make someone else
the host, remove someone, stop the title, end the party. If the host is disconnected for 30
seconds, the relay makes the next person the host.

### Leaving

Leaving the player tells the party not to wait for you; "Back to the party" in the panel reopens
the player. "Leave the party" leaves for good. Parties close on their own after 24 hours.

## App skins and player skins

- App skins can place the watch party button with `data-kn-component="watch-party"`
  ([app-skins.md](app-skins.md)). It renders nothing while the module is off.
- Player skins can open the panel with `data-kn-action="watch-party"`
  ([player-skins.md](player-skins.md)).

## Clients

The web app supports watch parties. The Android TV app doesn't yet: the plan is the OWP Kotlin SDK
(`party.openwatch:openwatchparty`) over Media3, using the same server endpoints below.

## API

| Endpoint | Who | What |
|---|---|---|
| `GET /api/watch-party` | signed in | `{enabled, relay_url, client}`; `relay_url` is null for the public relay. |
| `GET /api/watch-party/items/{id}` | signed in | A movie or episode as the party sees it (IDs, show, numbers, duration, size). |
| `POST /api/watch-party/find` | signed in | Items sharing a catalogue ID (`ids`), or episodes of a series sharing one (`series_ids`, `season`, `episode`). |
| `GET /api/watch-party/search?q=&kind=movie\|episode` | signed in | The title search; episodes by their show's title. |

All but the first answer 404 while the module is off. The party's details come from other people
through the relay, so the server only uses them as bound query values (catalogue namespaces are
limited to the ones OWP defines), and the web app shows them as plain text.
