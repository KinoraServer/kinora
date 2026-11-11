import { checkMedia, type Media } from "openwatchparty";
import { describe as group, expect, it } from "vitest";
import { describe, fold, itemLabel, libraryResolver, sameName, type PartyItem, type PartyLibrary } from "./resolver";

const show = { title: "Breaking Bad", year: 2008, ids: { tmdb: "1396" } };

function episode(id: string, season: number, ep: number, extra: Partial<PartyItem> = {}): PartyItem {
  return {
    id,
    kind: "episode",
    title: `Episode ${ep}`,
    year: null,
    ids: {},
    show,
    season,
    episode: ep,
    duration_ms: 3_487_000,
    size_bytes: 1000,
    ...extra,
  };
}

function movie(id: string, title: string, year: number, extra: Partial<PartyItem> = {}): PartyItem {
  return {
    id,
    kind: "movie",
    title,
    year,
    ids: {},
    show: null,
    season: null,
    episode: null,
    duration_ms: 8_880_000,
    size_bytes: null,
    ...extra,
  };
}

function library(byIds: PartyItem[], search: PartyItem[]): PartyLibrary & { searched: Media[] } {
  const known = new Map<string, PartyItem>();
  const searched: Media[] = [];
  return {
    searched,
    findByIds: async () => byIds,
    search: async (m) => {
      searched.push(m);
      return search;
    },
    canonical: (i) => {
      if (!known.has(i.id)) known.set(i.id, i);
      return known.get(i.id)!;
    },
  };
}

const signal = new AbortController().signal;

group("describe", () => {
  it("builds a descriptor the relay accepts, without file names", () => {
    const m = describe(episode("e1", 1, 1, { ids: { tmdb: "62085" } }), 3_487_000.4);
    expect(checkMedia(m)).toBeNull();
    expect(m).toMatchObject({
      kind: "episode",
      title: "Episode 1",
      durationMs: 3_487_000,
      series: { title: "Breaking Bad", year: 2008, ids: { tmdb: "1396" } },
      season: 1,
      episode: 1,
      file: { sizeBytes: 1000 },
    });
    expect(m.file?.name).toBeUndefined();
  });

  it("gives untitled episodes a stand-in title and leaves out empty ids", () => {
    const m = describe(episode("e5", 1, 5, { title: "  ", show: { title: "Show", year: null, ids: {} } }), 1000);
    expect(m.title).toBe("Episode 5");
    expect(m.ids).toBeUndefined();
    expect(m.series?.ids).toBeUndefined();
    expect(checkMedia(m)).toBeNull();
  });
});

group("sameName", () => {
  it("matches movies by folded title and a close year", () => {
    const media: Media = { kind: "movie", title: "Spider-Man: No Way Home", year: 2021, durationMs: 1 };
    expect(sameName(media, movie("m", "Spider Man  No Way Home", 2022))).toBe(true);
    expect(sameName(media, movie("m", "Spider Man No Way Home", 2019))).toBe(false);
    expect(
      sameName({ ...media, ids: { tmdb: "1" } }, movie("m", "Spider-Man: No Way Home", 2021, { ids: { tmdb: "2" } })),
    ).toBe(false);
  });

  it("matches episodes by show, season and episode", () => {
    const media: Media = {
      kind: "episode",
      title: "Pilot",
      series: { title: "breaking bad" },
      season: 1,
      episode: 1,
      durationMs: 1,
    };
    expect(sameName(media, episode("e", 1, 1))).toBe(true);
    expect(sameName(media, episode("e", 1, 2))).toBe(false);
  });

  it("folds case and punctuation", () => {
    expect(fold("  Ocean's   ELEVEN! ")).toBe("ocean s eleven");
  });
});

group("libraryResolver", () => {
  const room: Media = {
    kind: "episode",
    title: "Pilot",
    durationMs: 3_487_000,
    ids: { imdb: "tt0959621" },
    series: { title: "Breaking Bad", year: 2008, ids: { tmdb: "1396" } },
    season: 1,
    episode: 1,
  };

  it("puts catalogue-ID matches first and skips title matches when an ID matched", async () => {
    const byId = episode("id-match", 1, 1);
    const byName = episode("name-match", 1, 1, { show: { ...show, ids: {} } });
    const lib = library([byId], [byName, episode("other", 1, 2)]);
    const out = await libraryResolver(lib).resolve(room, signal);
    expect(out[0]).toMatchObject({ id: "id-match", durationMs: 3_487_000 });
    // Title matches and near misses are still offered, but without a duration the SDK never picks them itself.
    expect(out.slice(1).map((c) => c.id)).toEqual(["name-match", "other"]);
    expect(out[1].durationMs).toBeUndefined();
  });

  it("drops ID finds that aren't the same title", async () => {
    const lib = library([episode("wrong-ep", 1, 2)], []);
    expect(await libraryResolver(lib).resolve(room, signal)).toEqual([]);
  });

  it("falls back to the title search, by the show's title", async () => {
    const lib = library([], [episode("e", 1, 1, { show: { ...show, ids: {} } }), movie("m", "Breaking Bad", 2008)]);
    const out = await libraryResolver(lib).resolve(room, signal);
    expect(out.map((c) => [c.id, c.durationMs])).toEqual([["e", 3_487_000]]);
    expect(lib.searched[0].series?.title).toBe("Breaking Bad");
  });

  it("returns the same ref object for the same item", async () => {
    const lib = library([episode("e", 1, 1)], [episode("e", 1, 1)]);
    const first = await libraryResolver(lib).resolve(room, signal);
    const again = await libraryResolver(lib).resolve(room, signal);
    expect(again[0].ref).toBe(first[0].ref);
  });
});

it("labels items as plain text", () => {
  expect(itemLabel(episode("e", 2, 3, { title: "Bit by a Dead Bee" }))).toBe("Breaking Bad · S2E3 · Bit by a Dead Bee");
  expect(itemLabel(movie("m", "Heat", 1995))).toBe("Heat (1995)");
});
