// The starter app skins (Marquee, Blank) as bundles, read straight from the
// server's copies, plus the API answers a skinned page needs. Used by the
// app skin stories, so editing a starter shows up in Storybook right away.

import type { FeaturedItem, HomeRow, ItemCard, ItemDetail } from "@/lib/types";
import type { SkinBundle, SkinOptionDef } from "@/skin-engine/types";
import type { MockApi } from "./mock-api";
import { demoEpisodes, demoMovies } from "./fixtures";

const files = import.meta.glob<string>("../../../crates/server/src/modules/app_skin_samples/*/**/*", {
  query: "?raw",
  import: "default",
  eager: true,
});

export const STARTERS = ["marquee", "blank"] as const;
export type Starter = (typeof STARTERS)[number];

/** Build the bundle the server would send for a starter skin. */
export function starterBundle(name: Starter, optionValues: Record<string, string> = {}): SkinBundle {
  const prefix = `../../../crates/server/src/modules/app_skin_samples/${name}/`;
  const mine = Object.entries(files)
    .filter(([p]) => p.startsWith(prefix))
    .map(([p, text]) => [p.slice(prefix.length), text] as const);
  const get = (f: string) => mine.find(([p]) => p === f)?.[1] ?? "";
  const manifest = JSON.parse(get("skin.json")) as {
    id: string;
    name: string;
    version: string;
    devices?: string[];
    scheme?: "dark" | "light";
    theme?: string;
    options?: Record<string, SkinOptionDef>;
  };
  const templates: Record<string, string> = {};
  for (const [p, text] of mine) {
    const stem = p.replace(/\.html$/, "");
    if (p.endsWith(".html") && (stem === "shell" || stem.startsWith("pages/") || stem.startsWith("partials/"))) {
      templates[stem] = text;
    }
  }
  const defaults = Object.fromEntries(
    Object.entries(manifest.options ?? {}).map(([k, o]) => [k, o.default == null ? "" : String(o.default)]),
  );
  return {
    id: manifest.id,
    name: manifest.name,
    version: manifest.version,
    skinApi: 1,
    devices: manifest.devices ?? [],
    scheme: manifest.scheme ?? null,
    theme: manifest.theme ?? null,
    player: null,
    options: manifest.options ?? {},
    optionValues: { ...defaults, ...optionValues },
    templates,
    css: get("skin.css"),
    assets: [],
    hash: `${name}-${Object.values(optionValues).join(",")}`,
  };
}

const NOW = Date.now();
const art = (c: ItemCard, i: number): ItemCard => ({ ...c, added_at: NOW - i * 3_600_000 });

const featured: FeaturedItem[] = demoMovies.slice(0, 3).map((m, i) => ({
  ...art(m, i),
  overview: "A quiet, tense story about the people we keep and the ones we let go.",
  tagline: "Every light has a shadow.",
  official_rating: "PG-13",
  runtime_minutes: 112,
  genres: ["Drama", "Mystery"],
}));

/** A long episode name: its one-line subtitle must not stretch its card. */
const longEpisode: ItemCard = {
  ...demoEpisodes[0],
  id: "e-long",
  title: "Living in the Same Place, but Not Living Together, and Other Long Episode Names",
  series_title: "More than a Married Couple, but Not Lovers",
};

const home: HomeRow[] = [
  {
    id: "continue",
    title: "Continue watching",
    items: [demoMovies[3], longEpisode, ...demoEpisodes],
  },
  { id: "latest-lib-movies", title: "Recently added in Movies", items: demoMovies.map(art) },
];

export const demoSeries: ItemDetail = {
  ...demoMovies[0],
  id: "s1",
  kind: "series",
  title: "Signal",
  year: 2021,
  original_title: null,
  episode_end: null,
  overview: "A radio operator hears a voice that shouldn't exist.",
  tagline: null,
  official_rating: "TV-14",
  genres: ["Drama", "Sci-Fi"],
  runtime_minutes: 45,
  release_date: null,
  metadata_status: "matched",
  series_title: null,
  files: [],
  children: [
    { ...demoEpisodes[0], id: "season-1", kind: "season", title: "Season 1", season_number: 1, episode_number: null },
    { ...demoEpisodes[0], id: "season-2", kind: "season", title: "Season 2", season_number: 2, episode_number: null },
  ],
  position_ms: 0,
  played: false,
  next_up: demoEpisodes[0],
};

/** What a skinned app asks the server for. */
export function skinApi(bundle: SkinBundle): MockApi {
  return {
    "api/app-skin/active": bundle,
    "POST api/app-skins/errors": {},
    "api/home": home,
    "api/home/featured": featured,
    "api/items": demoMovies.map(art),
    "api/items/s1": demoSeries,
  };
}
