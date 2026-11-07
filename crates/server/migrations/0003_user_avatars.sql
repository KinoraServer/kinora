-- Profile pictures. Kept out of `users` so loading a user never drags the
-- image along. The web app shrinks pictures to 256x256 before uploading.

CREATE TABLE user_avatars (
    user_id      TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    content_type TEXT NOT NULL,            -- image/png | image/jpeg | image/webp | image/gif
    data         BLOB NOT NULL,
    updated_at   INTEGER NOT NULL
);
