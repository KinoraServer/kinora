//! The starter app skins (Marquee and Blank), compiled into the server and
//! written to `<config>/app-skins/*.sample` on first start.

macro_rules! files {
    ($dir:literal: $($path:literal),* $(,)?) => {
        &[$(($path, include_str!(concat!("app_skin_samples/", $dir, "/", $path)))),*]
    };
}

pub const MARQUEE: &[(&str, &str)] = files!("marquee":
    "skin.json", "README.md", "shell.html", "skin.css",
    "pages/home.html", "pages/home.tv.html", "pages/library.html", "pages/item.html", "pages/item.series.html",
    "pages/search.html",
    "partials/card.html", "partials/card-wide.html", "partials/poster.html", "partials/episode.html",
    "player/skin.html", "player/skin.css",
    "themes/marquee-dark.json",
);

pub const BLANK: &[(&str, &str)] = files!("blank":
    "skin.json", "README.md", "shell.html", "skin.css",
    "pages/home.html", "pages/library.html", "pages/item.html", "pages/search.html",
    "partials/card.html",
);
