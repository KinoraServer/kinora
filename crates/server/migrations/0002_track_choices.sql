-- Module "auto-tracks": the audio and subtitle tracks each viewer last picked,
-- remembered per season, per show and per library. `choice` is a JSON
-- kinora_core::tracks::TrackChoice (tracks described by language, title,
-- codec and flags, never by stream index).

CREATE TABLE track_choices (
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    scope      TEXT NOT NULL,              -- season | series | library
    scope_id   TEXT NOT NULL,              -- items.id or libraries.id
    choice     TEXT NOT NULL,
    item_id    TEXT,                       -- the episode or movie it was picked on
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, scope, scope_id)
);
