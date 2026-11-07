// View models: what a skin template sees. Skins never read API JSON
// directly; these builders turn it into the stable, pre-formatted shapes in
// skin-rules.json ("types"), so the API can change without breaking skins.
// Everything here is pure (and unit tested); the hooks below fetch the data.

import { useMemo } from "react";
import { avatarUrl } from "@/lib/api";
import { formatBytes, formatDuration } from "@/lib/format";
import { useApi, useSession } from "@/lib/session";
import type { FeaturedItem, HomeRow, ItemCard, ItemDetail, Library, MediaFile, SystemInfo, User } from "@/lib/types";
import type {
  AppVM,
  CardVM,
  Device,
  FileVM,
  HomeVM,
  InputMode,
  ItemVM,
  LibraryLinkVM,
  LibraryVM,
  RowVM,
  SearchVM,
  SeasonTabVM,
} from "./types";

/** How long a title counts as "new". */
const NEW_FOR_MS = 14 * 24 * 60 * 60 * 1000;

type AnyItem = Partial<ItemCard> &
  Partial<Pick<FeaturedItem, "overview" | "tagline" | "official_rating" | "runtime_minutes" | "genres">> & {
    id: string;
    kind: ItemCard["kind"];
    title: string;
    files?: MediaFile[];
  };

/** "S01E02" -> "S1 · E2" reads better on a card. */
export function shortCode(season: number | null | undefined, episode: number | null | undefined): string {
  return season == null || episode == null ? "" : `S${season} · E${episode}`;
}

export function toCard(it: AnyItem, now = Date.now()): CardVM {
  const isEpisode = it.kind === "episode";
  const duration =
    it.duration_ms ?? it.files?.[0]?.duration_ms ?? (it.runtime_minutes ? it.runtime_minutes * 60000 : 0);
  const position = it.position_ms ?? 0;
  const played = !!it.played;
  const inProgress = !played && position > 0;
  const progress = duration > 0 ? Math.min(1, Math.max(0, position / duration)) : 0;
  const genres = it.genres ?? [];
  const title = isEpisode && it.series_title ? it.series_title : it.title;
  const epCode = isEpisode ? shortCode(it.season_number, it.episode_number) : "";
  return {
    $type: "Card",
    id: it.id,
    kind: it.kind,
    title,
    subtitle: isEpisode
      ? [epCode, it.title].filter(Boolean).join(" · ")
      : it.kind === "season"
        ? (it.series_title ?? "")
        : it.year
          ? String(it.year)
          : it.kind === "series"
            ? "Series"
            : "",
    seriesTitle: it.series_title ?? "",
    episodeTitle: isEpisode ? it.title : "",
    episodeCode: epCode,
    year: it.year ? String(it.year) : "",
    rating: it.rating ? it.rating.toFixed(1) : "",
    officialRating: it.official_rating ?? "",
    runtimeLabel: formatDuration(duration),
    progress,
    remainingLabel: inProgress && duration > position ? `${formatDuration(duration - position) || "1m"} left` : "",
    played,
    inProgress,
    isNew: !!it.added_at && now - it.added_at < NEW_FOR_MS && !played && !inProgress,
    isPlayable: it.kind === "movie" || isEpisode,
    hasPoster: !!it.has_poster,
    hasBackdrop: !!it.has_backdrop,
    hasThumb: !!it.has_thumb,
    hasLogo: !!it.has_logo,
    overview: it.overview ?? "",
    tagline: it.tagline ?? "",
    genres,
    genresLabel: genres.slice(0, 3).join(" · "),
    parentId: it.parent_id ?? "",
    seriesId: it.series_id ?? "",
    $positionMs: inProgress ? position : 0,
  } as CardVM;
}

export function rowKind(id: string): RowVM["kind"] {
  if (id === "continue") return "continue";
  if (id === "next-up") return "next-up";
  if (id.startsWith("recent-")) return "latest";
  return "library";
}

export function buildHome(rows: HomeRow[] | null, featured: FeaturedItem[] | null, now = Date.now()): HomeVM {
  const r: RowVM[] = (rows ?? []).map((row) => ({
    $type: "Row",
    id: row.id,
    title: row.title,
    kind: rowKind(row.id),
    items: row.items.map((i) => toCard(i, now)),
  })) as RowVM[];
  return {
    $type: "Home",
    rows: r,
    featured: (featured ?? []).map((f) => toCard(f, now)),
    isEmpty: r.length === 0,
  } as HomeVM;
}

export function toLibraryLink(lib: Library, activeId?: string | null): LibraryLinkVM {
  return {
    $type: "LibraryLink",
    id: lib.id,
    name: lib.name,
    kind: lib.kind,
    itemCount: lib.item_count,
    active: lib.id === activeId,
  } as LibraryLinkVM;
}

export function buildApp(args: {
  info: SystemInfo | null;
  user: User | null;
  isAdmin: boolean;
  libraries: Library[] | null;
  path: string;
  query: string;
  options: Record<string, string>;
  device: Device;
  input: InputMode;
}): AppVM {
  const { info, user, libraries, path } = args;
  const activeLibrary = /^\/library\/([^/?]+)/.exec(path)?.[1] ?? null;
  const routeName = path === "/" ? "home" : (/^\/([a-z]+)/.exec(path)?.[1] ?? "home");
  return {
    $type: "App",
    server: { name: info?.server_name ?? "Kinora", version: info?.version ?? "" },
    user: {
      id: user?.id ?? "",
      name: user?.username ?? "",
      initial: (user?.username ?? "?").slice(0, 1).toUpperCase(),
      isAdmin: args.isAdmin,
      hasAvatar: !!user?.avatar_version,
      $avatarUrl: user ? avatarUrl(user) : null,
    },
    libraries: (libraries ?? []).map((l) => toLibraryLink(l, activeLibrary)),
    route: { name: routeName, path },
    search: { query: args.query },
    options: args.options,
    device: args.device,
    input: args.input,
  } as AppVM;
}

export const SORTS = [
  { value: "title", label: "Title" },
  { value: "added", label: "Recently added" },
  { value: "year", label: "Year" },
  { value: "rating", label: "Rating" },
];

export function buildLibrary(
  lib: Library | undefined,
  items: ItemCard[] | null,
  sort: string,
  loading: boolean,
): LibraryVM {
  const list = (items ?? []).map((i) => toCard(i));
  const count = list.length;
  return {
    $type: "Library",
    library: lib
      ? toLibraryLink(lib, lib.id)
      : ({ $type: "LibraryLink", id: "", name: "Library", kind: "", itemCount: 0, active: true } as LibraryLinkVM),
    items: list,
    count,
    countLabel: items ? `${count} ${count === 1 ? "title" : "titles"}` : "",
    isEmpty: !!items && count === 0,
    isLoading: loading,
    sort: SORTS.map((s) => ({ $type: "SortOption", ...s, active: s.value === sort })),
  } as LibraryVM;
}

export function toFile(f: MediaFile): FileVM {
  const v = f.info?.video;
  const audio = f.info?.audio ?? [];
  const subs = f.info?.subtitles ?? [];
  const lang = (l: string | null) => (l ? l.toUpperCase() : "");
  const hdr = v && v.hdr !== "sdr" ? (v.hdr === "dolby_vision" ? "Dolby Vision" : v.hdr.toUpperCase()) : "";
  return {
    $type: "File",
    name: f.path,
    sizeLabel: formatBytes(f.size),
    container: f.info?.container ?? "",
    resolution: v ? `${v.width}×${v.height}` : "",
    videoLabel: v
      ? [`${v.codec.toUpperCase()} ${v.width}×${v.height}`, `${v.bit_depth}-bit`, hdr].filter(Boolean).join(" · ")
      : "",
    videoCodec: v?.codec ?? "",
    audioLabel: audio
      .map((a) => [lang(a.language), a.codec.toUpperCase(), `${a.channels}ch`].filter(Boolean).join(" "))
      .join(", "),
    subtitleLabel: subs.length ? `Subtitles: ${subs.map((s) => lang(s.language) || s.codec).join(", ")}` : "",
    hdr,
  } as FileVM;
}

/**
 * The item page. For a series, `seasonId` picks the season whose episodes
 * show (default: the season of the next episode, else the first);
 * `seasonEpisodes` are that season's episodes, fetched separately.
 */
export function buildItem(
  item: ItemDetail,
  seasonId: string | null,
  seasonEpisodes: ItemCard[] | null,
  now = Date.now(),
): ItemVM {
  const card = toCard(
    {
      ...item,
      // On its own page an item is called by its own name; episodes keep the series title.
      duration_ms: item.files[0]?.duration_ms ?? (item.runtime_minutes ? item.runtime_minutes * 60000 : null),
    },
    now,
  );
  if (item.kind === "season") card.title = item.title;
  const children = item.children.map((c) => toCard(c, now));
  const seasonCards = item.kind === "series" ? item.children.filter((c) => c.kind === "season") : [];
  const nextUp = item.next_up ? toCard(item.next_up, now) : null;
  const wanted = seasonId ?? item.next_up?.parent_id ?? seasonCards[0]?.id ?? null;
  const activeSeason = seasonCards.find((s) => s.id === wanted) ?? seasonCards[0];
  const seasons: SeasonTabVM[] = seasonCards.map((s) => ({
    $type: "SeasonTab",
    id: s.id,
    label: s.title,
    number: s.season_number ?? 0,
    active: s.id === activeSeason?.id,
  })) as SeasonTabVM[];
  const episodes =
    item.kind === "series"
      ? (seasonEpisodes ?? []).map((e) => toCard(e, now))
      : item.kind === "season"
        ? children.filter((c) => c.kind === "episode")
        : [];
  const playable = item.kind === "movie" || item.kind === "episode";
  const canResume = playable && card.inProgress;
  const playTarget = playable ? card : nextUp;
  const playLabel = canResume
    ? "Resume"
    : playable
      ? "Play"
      : nextUp
        ? `Play ${shortCode(item.next_up?.season_number, item.next_up?.episode_number)}`.trim()
        : "Play";
  return {
    ...card,
    $type: "Item",
    originalTitle: item.original_title ?? "",
    releaseDate: item.release_date ?? "",
    files: item.files.map(toFile),
    hasFiles: item.files.length > 0,
    children,
    seasons,
    hasSeasons: seasons.length > 0,
    season: seasons.find((s) => s.active) ?? null,
    episodes,
    hasEpisodes: episodes.length > 0,
    nextUp,
    canPlay: !!playTarget,
    canResume,
    playLabel,
    resumeLabel: canResume ? "Resume" : "",
    playedLabel: item.played ? "Mark unwatched" : "Mark watched",
  } as ItemVM;
}

export function buildSearch(query: string, results: ItemCard[] | null, loading: boolean): SearchVM {
  const list = (results ?? []).map((r) => toCard(r));
  const hasQuery = query.trim().length > 0;
  return {
    $type: "Search",
    query,
    hasQuery,
    results: hasQuery ? list : [],
    isEmpty: hasQuery && !!results && list.length === 0 && !loading,
    isSearching: hasQuery && loading,
    countLabel: hasQuery && results ? `${list.length} ${list.length === 1 ? "result" : "results"}` : "",
  } as SearchVM;
}

// ---------------------------------------------------------------------------
// Hooks
// ---------------------------------------------------------------------------

export function useAppVM(
  path: string,
  query: string,
  options: Record<string, string>,
  device: Device,
  input: InputMode,
) {
  const { info, user, isAdmin } = useSession();
  const { data: libraries } = useApi<Library[]>("api/libraries");
  return useMemo(
    () => buildApp({ info, user, isAdmin, libraries, path, query, options, device, input }),
    [info, user, isAdmin, libraries, path, query, options, device, input],
  );
}

export function useHomeVM() {
  const rows = useApi<HomeRow[]>("api/home");
  const featured = useApi<FeaturedItem[]>("api/home/featured");
  const vm = useMemo(() => buildHome(rows.data, featured.data), [rows.data, featured.data]);
  return {
    vm,
    loading: (rows.loading && !rows.data) || (featured.loading && !featured.data),
    error: rows.error,
    reload: () => {
      rows.reload();
      featured.reload();
    },
  };
}

export function useLibraryVM(id: string, sort: string) {
  const { data: libraries } = useApi<Library[]>("api/libraries");
  const items = useApi<ItemCard[]>(`api/items?library_id=${encodeURIComponent(id)}&sort=${encodeURIComponent(sort)}`, [
    id,
    sort,
  ]);
  const lib = libraries?.find((l) => l.id === id);
  const vm = useMemo(() => buildLibrary(lib, items.data, sort, items.loading), [lib, items.data, sort, items.loading]);
  return { vm, loading: items.loading && !items.data, error: items.error, reload: items.reload };
}

export function useItemVM(id: string, seasonParam: string | null) {
  const item = useApi<ItemDetail>(`api/items/${encodeURIComponent(id)}`, [id]);
  const d = item.data && item.data.id === id ? item.data : null;
  const seasonId =
    d?.kind === "series"
      ? (seasonParam ?? d.next_up?.parent_id ?? d.children.find((c) => c.kind === "season")?.id ?? null)
      : null;
  const episodes = useApi<ItemCard[]>(seasonId ? `api/items?parent_id=${encodeURIComponent(seasonId)}` : null, [
    seasonId,
  ]);
  const vm = useMemo(
    () => (d ? buildItem(d, seasonId, seasonId ? episodes.data : null) : null),
    [d, seasonId, episodes.data],
  );
  return {
    vm,
    loading: item.loading && !d,
    error: item.error,
    reload: () => {
      item.reload();
      episodes.reload();
    },
  };
}

export function useSearchVM(query: string) {
  const q = query.trim();
  const res = useApi<ItemCard[]>(q ? `api/items?q=${encodeURIComponent(q)}` : null, [q]);
  const vm = useMemo(
    () => buildSearch(query, q ? res.data : null, !!q && res.loading),
    [query, q, res.data, res.loading],
  );
  return { vm, loading: false, error: res.error, reload: res.reload };
}
