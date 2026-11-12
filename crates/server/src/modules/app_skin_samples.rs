//! The Blank starter app skin, compiled into the server: `kinora skin new` and
//! "Start from Blank" copy it, and it is written to
//! `<config>/app-skins/blank.sample` on first start.
//!
//! Marquee is installed from the registry (Admin > Modules). The copy in
//! `app_skin_samples/marquee` is only what the tests and Storybook render the
//! skin engine with; it is not part of the server.

macro_rules! files {
    ($dir:literal: $($path:literal),* $(,)?) => {
        &[$(($path, include_str!(concat!("app_skin_samples/", $dir, "/", $path)))),*]
    };
}

#[cfg(test)]
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
