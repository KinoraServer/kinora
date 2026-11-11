// Matching a watch party's media to the viewer's own Kinora library.
//
// The room's media descriptor comes from another participant through the
// relay, so everything in it is untrusted: its values are only ever used as
// search terms and bound query values on the server, never as paths or URLs,
// and only ever shown as plain text.

import { sameTitle, type Candidate, type Media, type Resolver } from "openwatchparty";

/** One movie or episode with its main file, as `/api/watch-party/*` returns it. */
export interface PartyItem {
  id: string;
  kind: "movie" | "episode";
  title: string;
  year: number | null;
  ids: Record<string, string>;
  show: { title: string; year: number | null; ids: Record<string, string> } | null;
  season: number | null;
  episode: number | null;
  duration_ms: number | null;
  size_bytes: number | null;
}

/** The parts of Kinora's API the resolver uses. */
export interface PartyLibrary {
  /** Items holding any of these catalogue IDs, or episodes of a series holding one (same season and episode). */
  findByIds(media: Media, signal: AbortSignal): Promise<PartyItem[]>;
  /** Kinora's title search: movies by title, episodes by their show's title. */
  search(media: Media, signal: AbortSignal): Promise<PartyItem[]>;
  /** The same object for the same item id, so refs compare equal across calls. */
  canonical(item: PartyItem): PartyItem;
}

const NEAR_MISSES = 10;

const opt = <T>(v: T | null | undefined): T | undefined => (v === null ? undefined : v);
const nonEmpty = (ids: Record<string, string>) => (Object.keys(ids).length > 0 ? ids : undefined);

/** The descriptor Kinora sends for `item` as host. Titles are required, so an untitled episode gets "Episode N". */
export function describe(item: PartyItem, durationMs: number): Media {
  const title =
    item.title.trim() !== ""
      ? item.title
      : item.kind === "episode" && item.episode !== null
        ? `Episode ${item.episode}`
        : "Untitled";
  // No file name or path: nothing in a descriptor may locate the file, and names can say more than the title.
  return {
    kind: item.kind,
    title,
    year: opt(item.year),
    ids: nonEmpty(item.ids),
    series:
      item.kind === "episode" && item.show
        ? { title: item.show.title.trim() || title, year: opt(item.show.year), ids: nonEmpty(item.show.ids) }
        : undefined,
    season: opt(item.season),
    episode: opt(item.episode),
    durationMs: Math.round(durationMs),
    file: item.size_bytes ? { sizeBytes: item.size_bytes } : undefined,
  };
}

/** Lower case, with every run of characters other than letters and digits as one space. */
export function fold(s: string): string {
  return s
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, " ")
    .trim();
}

function closeYears(a: number | null | undefined, b: number | null | undefined): boolean {
  return a == null || b == null || Math.abs(a - b) <= 1;
}

/** Some namespace holds a different ID on each side: two different titles. */
function conflicts(a: Readonly<Record<string, string>> | undefined, b: Readonly<Record<string, string>>): boolean {
  return Object.entries(a ?? {}).some(([ns, id]) => b[ns] !== undefined && b[ns] !== id);
}

/** Step 2: the same title and year, or the same show, season and episode, with no ID saying otherwise. */
export function sameName(media: Media, item: PartyItem): boolean {
  if (item.kind !== media.kind) return false;
  if (media.kind === "movie") {
    return (
      fold(item.title) === fold(media.title) && closeYears(media.year, item.year) && !conflicts(media.ids, item.ids)
    );
  }
  return (
    item.show !== null &&
    media.series !== undefined &&
    fold(item.show.title) === fold(media.series.title) &&
    closeYears(media.series.year, item.show.year) &&
    media.season !== undefined &&
    media.season === item.season &&
    media.episode !== undefined &&
    media.episode === item.episode &&
    !conflicts(media.ids, item.ids)
  );
}

/** A candidate the SDK can match: Kinora knows every file's duration from the scan. */
export function playable(item: PartyItem): Candidate<PartyItem> {
  return {
    id: item.id,
    ref: item,
    durationMs: item.duration_ms ?? undefined,
    sizeBytes: item.size_bytes ?? undefined,
  };
}

/** "Show · S1E2 · Title", "Movie (2010)": how Kinora names an item or a descriptor. */
export function labelOf(x: {
  kind: string;
  title: string;
  year?: number | null;
  season?: number | null;
  episode?: number | null;
  series?: string | null;
}): string {
  if (x.kind === "episode") {
    const code = x.season != null && x.episode != null ? `S${x.season}E${x.episode}` : null;
    return [x.series, code, x.title.trim() || null].filter(Boolean).join(" · ");
  }
  return x.year != null ? `${x.title} (${x.year})` : x.title;
}

export const itemLabel = (item: PartyItem) => labelOf({ ...item, series: item.show?.title ?? null });

export const mediaLabel = (media: Media) => labelOf({ ...media, series: media.series?.title ?? null });

/**
 * The SDK's resolver over Kinora's library.
 *
 * 1. Items sharing a catalogue ID with the media or its series, whatever their titles (checked with `sameTitle`).
 * 2. Only when step 1 found nothing: the title search, kept to the same title and year, or the same show, season
 *    and episode. Durations decide between them.
 * 3. Near misses from the search fill the choose list; without a duration the SDK never picks them on its own.
 */
export function libraryResolver(library: PartyLibrary): Resolver<PartyItem> {
  return {
    async resolve(media, signal) {
      const out = new Map<string, Candidate<PartyItem>>();
      for (const found of await library.findByIds(media, signal)) {
        const item = library.canonical(found);
        if (!out.has(item.id) && item.kind === media.kind && sameTitle(media, describe(item, media.durationMs))) {
          out.set(item.id, playable(item));
        }
      }
      const found = (await library.search(media, signal)).map((i) => library.canonical(i));
      if (out.size === 0) {
        for (const item of found.filter((i) => sameName(media, i)))
          if (!out.has(item.id)) out.set(item.id, playable(item));
      }
      for (const item of found.filter((i) => i.kind === media.kind).slice(0, NEAR_MISSES)) {
        if (!out.has(item.id)) out.set(item.id, { id: item.id, ref: item, sizeBytes: item.size_bytes ?? undefined });
      }
      return [...out.values()];
    },
  };
}
