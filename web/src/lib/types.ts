// Shapes returned by the Kinora API.

export type Role = "owner" | "admin" | "editor" | "viewer";

export interface User {
  id: string;
  username: string;
  role: Role;
}

export interface SystemInfo {
  name: string;
  version: string;
  server_name: string;
  setup_required: boolean;
}

export interface ItemCard {
  id: string;
  library_id: string;
  kind: "movie" | "series" | "season" | "episode";
  parent_id: string | null;
  series_id: string | null;
  title: string;
  year: number | null;
  season_number: number | null;
  episode_number: number | null;
  series_title: string | null;
  has_poster: boolean;
  has_backdrop: boolean;
  has_thumb: boolean;
  duration_ms: number | null;
  position_ms: number | null;
  played: boolean | null;
  rating: number | null;
}

export interface HomeRow {
  id: string;
  title: string;
  items: ItemCard[];
}

export interface AudioStream {
  index: number;
  codec: string;
  channels: number;
  language: string | null;
  title: string | null;
  is_default: boolean;
}

export interface SubtitleStream {
  index: number;
  codec: string;
  language: string | null;
  title: string | null;
  is_text: boolean;
}

export interface MediaInfo {
  container: string;
  duration_ms: number;
  bitrate: number | null;
  video: {
    codec: string;
    width: number;
    height: number;
    bit_depth: number;
    hdr: "sdr" | "hdr10" | "hlg" | "dolby_vision";
    dv_profile: number | null;
  } | null;
  audio: AudioStream[];
  subtitles: SubtitleStream[];
}

export interface MediaFile {
  id: string;
  path: string;
  size: number;
  duration_ms: number | null;
  label: string | null;
  info: MediaInfo | null;
}

export interface ItemDetail extends Omit<ItemCard, "duration_ms" | "played" | "position_ms" | "series_title"> {
  original_title: string | null;
  episode_end: number | null;
  overview: string | null;
  tagline: string | null;
  official_rating: string | null;
  genres: string[];
  runtime_minutes: number | null;
  release_date: string | null;
  metadata_status: string;
  series_title: string | null;
  files: MediaFile[];
  children: ItemCard[];
  position_ms: number;
  played: boolean;
  next_up: ItemCard | null;
}

export interface Library {
  id: string;
  name: string;
  kind: "movies" | "tv";
  paths: string[];
  created_at: number;
  last_scan_at: number | null;
  item_count: number;
  scan: ScanStatus | null;
}

export interface ScanStatus {
  running: boolean;
  phase: string;
  processed: number;
  total: number;
  added: number;
  removed: number;
  errors: number;
  last_error: string | null;
}

export interface Theme {
  id: string;
  name: string;
  author?: string | null;
  scheme: "dark" | "light";
  tokens: Record<string, string>;
  player?: { layout?: string | null; options?: Record<string, string> } | null;
}

/** A custom player skin installed by the server admin (see docs/player-skins.md). */
export interface PlayerSkin {
  id: string;
  name: string;
  author?: string | null;
  html: string;
  css: string;
}

export interface PlaybackPlan {
  method: "direct_play" | "remux" | "transcode";
  reasons: string[];
  video: { kind: "copy" } | { kind: "encode"; width: number; height: number; bitrate: number; tonemap: boolean } | null;
  audio: { kind: "copy" } | { kind: "encode"; codec: string; channels: number } | null;
  audio_index: number | null;
}

export interface SubtitleTrack {
  index: number;
  label: string;
  language: string | null;
  url: string;
  codec: string;
  is_default: boolean;
  is_forced: boolean;
  /** false for image subtitles (PGS, VobSub), which can't be shown in the browser yet. */
  supported: boolean;
}

export interface PlaybackStart {
  method: PlaybackPlan["method"];
  url: string;
  session_id: string | null;
  plan: PlaybackPlan;
  duration_ms: number;
  subtitles: SubtitleTrack[];
  hardware: string;
}
