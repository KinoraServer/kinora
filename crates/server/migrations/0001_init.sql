-- All times are Unix milliseconds. All ids are UUIDv7 strings.

CREATE TABLE users (
    id            TEXT PRIMARY KEY,
    username      TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    role          TEXT NOT NULL,           -- owner | admin | editor | viewer
    created_at    INTEGER NOT NULL,
    last_login_at INTEGER
);

CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY,           -- sha256 of the token; the token itself is never stored
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device     TEXT,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE INDEX sessions_user ON sessions(user_id);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE modules (
    id       TEXT PRIMARY KEY,
    enabled  INTEGER NOT NULL,
    settings TEXT NOT NULL DEFAULT '{}'    -- JSON object of module settings
);

CREATE TABLE libraries (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    kind         TEXT NOT NULL,            -- movies | tv
    paths        TEXT NOT NULL,            -- JSON array of folders
    created_at   INTEGER NOT NULL,
    last_scan_at INTEGER
);

CREATE TABLE items (
    id              TEXT PRIMARY KEY,
    library_id      TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL,         -- movie | series | season | episode
    parent_id       TEXT REFERENCES items(id) ON DELETE CASCADE,
    series_id       TEXT,
    match_key       TEXT NOT NULL,         -- dedupe key used by the scanner
    title           TEXT NOT NULL,
    sort_title      TEXT NOT NULL,
    original_title  TEXT,
    year            INTEGER,
    season_number   INTEGER,
    episode_number  INTEGER,
    episode_end     INTEGER,
    overview        TEXT,
    tagline         TEXT,
    rating          REAL,
    official_rating TEXT,
    genres          TEXT NOT NULL DEFAULT '[]',
    runtime_minutes INTEGER,
    release_date    TEXT,
    provider_ids    TEXT NOT NULL DEFAULT '{}',
    poster_url      TEXT,
    backdrop_url    TEXT,
    thumb_url       TEXT,
    metadata_status TEXT NOT NULL DEFAULT 'pending',  -- pending | matched | unmatched
    added_at        INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);
CREATE UNIQUE INDEX items_match ON items(library_id, match_key);
CREATE INDEX items_parent ON items(parent_id);
CREATE INDEX items_library_kind ON items(library_id, kind, sort_title);
CREATE INDEX items_added ON items(kind, added_at);

CREATE TABLE media_files (
    id          TEXT PRIMARY KEY,
    item_id     TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    library_id  TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    path        TEXT NOT NULL UNIQUE,
    size        INTEGER NOT NULL,
    mtime       INTEGER NOT NULL,
    container   TEXT,
    duration_ms INTEGER,
    info        TEXT,                      -- JSON MediaInfo (streams, HDR, ...)
    scanned_at  INTEGER NOT NULL
);
CREATE INDEX media_files_item ON media_files(item_id);
CREATE INDEX media_files_library ON media_files(library_id);

CREATE TABLE user_item_data (
    user_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    item_id        TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    position_ms    INTEGER NOT NULL DEFAULT 0,
    played         INTEGER NOT NULL DEFAULT 0,
    play_count     INTEGER NOT NULL DEFAULT 0,
    last_played_at INTEGER,
    favorite       INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, item_id)
);
CREATE INDEX user_item_recent ON user_item_data(user_id, last_played_at);

CREATE TABLE activity_log (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    time    INTEGER NOT NULL,
    user_id TEXT,
    event   TEXT NOT NULL,
    payload TEXT NOT NULL
);
CREATE INDEX activity_time ON activity_log(time);
