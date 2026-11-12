# Auto audio & subtitles

Module `auto-tracks`: install "Auto audio & subtitles" from the list in Admin → Modules & themes
(see [modules.md](modules.md)). It does what the Plex-Auto-Languages tool does for Plex, but
inside the server, so there is nothing extra to run.

## What it does

- When a viewer changes the audio or subtitle track in the player, the client sends the pair to
  `PUT /api/playback/{file_id}/tracks`. The module saves a description of the tracks (language,
  title, codec, channels, forced and SDH flags), never the stream index, because indexes differ
  between files.
- The choice is saved for that viewer for the episode's season, its show and its library (a movie
  only has the library).
- When playback starts, `POST /api/playback/{file_id}` looks the choice up and returns
  `auto_tracks` with the closest tracks in the new file. The audio pick is used for the playback
  plan right away; the client shows the subtitle pick.

Choices are per viewer and live on the server, so every client (web, Android) gets the same picks.
New episodes need no scan step: tracks are picked when they start.

## Lookup order

1. The season's choice, if "Remember per season" is on (off by default).
2. The show's choice.
3. The library's last choice, if "Use the library's last choice for new shows" is on (on by
   default). This is what makes an anime library start new shows in Japanese with English
   subtitles.

A choice is skipped if nothing in the new file matches it, and the next one is tried.

## Matching

The rules live in the module (`packages/auto-tracks` in the kinora-registry repository) with
unit tests.

- The language must match (`en` and `eng`, `ger` and `deu` count as the same). Otherwise there is
  no opinion and the client uses the file's default track.
- Subtitles: same forced flag (+8), same title (+6) or a title sharing a meaningful word (+3),
  same SDH flag (+4), same codec (+1).
- Audio: same title (+4), codec (+2) and channel count (+2).
- Commentary tracks are only picked when the viewer picked commentary.
- Ties go to the track that comes first in the file.
- Subtitles "Off" is remembered as off.

## API

| Call                                 | What                                                        |
| ------------------------------------ | ----------------------------------------------------------- |
| `PUT /api/playback/{file_id}/tracks` | `{ "audio_index": 2, "subtitle_index": 5 }` (null = off)    |
| `POST /api/playback/{file_id}`       | response `auto_tracks: { from, audio_index, subtitle_index? }` |
| `DELETE /api/track-choices`          | forget the signed-in viewer's choices                       |

`subtitle_index` missing means no opinion; `null` means subtitles off. Passing `audio_index` when
starting playback skips the lookup.
