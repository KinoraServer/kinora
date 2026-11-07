// Shapes used by the app skin engine. The view models (Card, Home, ...) match
// the "types" in skin-rules.json; docs/app-skins.md lists them for authors.

/** `GET /api/app-skin/active` and `/api/app-skins/{id}/bundle`. */
export interface SkinBundle {
  id: string;
  name: string;
  version: string;
  author?: string | null;
  skinApi: number;
  devices: string[];
  scheme?: "dark" | "light" | null;
  theme?: string | null;
  player?: { style: string; bundled: boolean } | null;
  options: Record<string, SkinOptionDef>;
  optionValues: Record<string, string>;
  templates: Record<string, string>;
  css: string;
  assets: string[];
  hash: string;
}

export interface SkinOptionDef {
  type: "bool" | "enum" | "number" | "color";
  label: string;
  default?: string | number | boolean | null;
  values?: string[];
  min?: number;
  max?: number;
}

/** A compiled template node. */
export type SkinNode = SkinText | SkinElement;

export interface SkinText {
  t: "text";
  v: string;
}

export interface SkinElement {
  t: "el";
  tag: string;
  /** Safe static attributes (already namespaced and rewritten). */
  attrs: Record<string, string>;
  /** data-kn-* bindings, by name without the prefix: { each: "rows", text: "title" }. */
  kn: Record<string, string>;
  children: SkinNode[];
}

export type Device = "phone" | "tablet" | "desktop" | "tv";
export type InputMode = "pointer" | "touch" | "remote";

/** A compiled skin, ready to render. */
export interface CompiledSkin {
  bundle: SkinBundle;
  templates: Map<string, SkinNode[]>;
  css: string;
}

// ---------------------------------------------------------------------------
// View models
// ---------------------------------------------------------------------------

export interface CardVM {
  /** Which contract type this is (used to find the item a link or action is about). */
  $type?: string;
  id: string;
  kind: "movie" | "series" | "season" | "episode";
  title: string;
  subtitle: string;
  seriesTitle: string;
  episodeTitle: string;
  episodeCode: string;
  year: string;
  rating: string;
  officialRating: string;
  runtimeLabel: string;
  progress: number;
  remainingLabel: string;
  played: boolean;
  inProgress: boolean;
  isNew: boolean;
  isPlayable: boolean;
  hasPoster: boolean;
  hasBackdrop: boolean;
  hasThumb: boolean;
  hasLogo: boolean;
  overview: string;
  tagline: string;
  genres: string[];
  genresLabel: string;
  parentId: string;
  seriesId: string;
  /** Saved position in ms (not part of the contract; used by play links). */
  $positionMs: number;
}

export interface RowVM {
  /** Which contract type this is (used to find the item a link or action is about). */
  $type?: string;
  id: string;
  title: string;
  kind: "continue" | "latest" | "library" | "next-up";
  items: CardVM[];
}

export interface LibraryLinkVM {
  /** Which contract type this is (used to find the item a link or action is about). */
  $type?: string;
  id: string;
  name: string;
  kind: string;
  itemCount: number;
  active: boolean;
}

export interface AppVM {
  /** Which contract type this is (used to find the item a link or action is about). */
  $type?: string;
  server: { name: string; version: string };
  user: { id: string; name: string; initial: string; isAdmin: boolean; hasAvatar: boolean; $avatarUrl: string | null };
  libraries: LibraryLinkVM[];
  route: { name: string; path: string };
  search: { query: string };
  options: Record<string, string>;
  device: Device;
  input: InputMode;
}

export interface HomeVM {
  /** Which contract type this is (used to find the item a link or action is about). */
  $type?: string;
  rows: RowVM[];
  featured: CardVM[];
  isEmpty: boolean;
}

export interface SortOptionVM {
  /** Which contract type this is (used to find the item a link or action is about). */
  $type?: string;
  value: string;
  label: string;
  active: boolean;
}

export interface LibraryVM {
  /** Which contract type this is (used to find the item a link or action is about). */
  $type?: string;
  library: LibraryLinkVM;
  items: CardVM[];
  count: number;
  countLabel: string;
  isEmpty: boolean;
  isLoading: boolean;
  sort: SortOptionVM[];
}

export interface FileVM {
  /** Which contract type this is (used to find the item a link or action is about). */
  $type?: string;
  name: string;
  sizeLabel: string;
  container: string;
  resolution: string;
  videoLabel: string;
  videoCodec: string;
  audioLabel: string;
  subtitleLabel: string;
  hdr: string;
}

export interface SeasonTabVM {
  /** Which contract type this is (used to find the item a link or action is about). */
  $type?: string;
  id: string;
  label: string;
  number: number;
  active: boolean;
}

export interface ItemVM extends CardVM {
  originalTitle: string;
  releaseDate: string;
  files: FileVM[];
  hasFiles: boolean;
  children: CardVM[];
  seasons: SeasonTabVM[];
  hasSeasons: boolean;
  season: SeasonTabVM | null;
  episodes: CardVM[];
  hasEpisodes: boolean;
  nextUp: CardVM | null;
  canPlay: boolean;
  canResume: boolean;
  playLabel: string;
  resumeLabel: string;
  playedLabel: string;
}

export interface SearchVM {
  /** Which contract type this is (used to find the item a link or action is about). */
  $type?: string;
  query: string;
  hasQuery: boolean;
  results: CardVM[];
  isEmpty: boolean;
  isSearching: boolean;
  countLabel: string;
}

export type PageName = "home" | "library" | "item" | "search";
