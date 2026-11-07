-- App skins (docs/app-skins.md): per-user preferences and title art.

-- Small per-user settings, such as a viewer's own app skin.
CREATE TABLE user_prefs (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key     TEXT NOT NULL,
    value   TEXT NOT NULL,
    PRIMARY KEY (user_id, key)
);

-- Title art (a transparent logo of the title) from the metadata provider.
ALTER TABLE items ADD COLUMN logo_url TEXT;
