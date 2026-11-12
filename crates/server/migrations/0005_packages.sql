-- Packages installed from a registry (Admin > Modules): modules, themes,
-- player skins and app skins. `manifest` is the package.json it was installed
-- from (kinora_plugin_api::registry::PackageManifest).

CREATE TABLE packages (
    id           TEXT PRIMARY KEY,
    kind         TEXT NOT NULL,            -- module | theme | player-skin | app-skin
    version      TEXT NOT NULL,
    manifest     TEXT NOT NULL,
    registry     TEXT NOT NULL,            -- the registry URL it came from
    installed_at INTEGER NOT NULL
);

-- What installed modules store for themselves (host functions kv_*). A row
-- with a user belongs to that viewer and goes when they do.

CREATE TABLE module_data (
    module_id  TEXT NOT NULL,
    user_id    TEXT REFERENCES users(id) ON DELETE CASCADE,
    key        TEXT NOT NULL,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX module_data_key ON module_data (module_id, COALESCE(user_id, ''), key);

-- Auto audio & subtitles is now an installed module that keeps its choices in
-- module_data. Its `choice` JSON is unchanged.

INSERT INTO module_data (module_id, user_id, key, value, updated_at)
SELECT 'auto-tracks', user_id, 'choice:' || scope || ':' || scope_id, choice, updated_at FROM track_choices;

DROP TABLE track_choices;
