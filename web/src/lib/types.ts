// Shapes returned by the Kinora API.

export type Role = "owner" | "admin" | "editor" | "viewer";

export interface User {
  id: string;
  username: string;
  role: Role;
  /** When the profile picture last changed (Unix ms); null without one. */
  avatar_version?: number | null;
}

export interface SystemInfo {
  name: string;
  version: string;
  server_name: string;
  setup_required: boolean;
  /** App skin contract versions this server supports. */
  skin_api?: { current: number; supported: number[] };
}

/** `GET /api/system/settings`: the whole object is read and written together. */
export interface SystemSettings {
  server_name: string;
  default_theme: string;
  hw_accel: string;
  /** Minutes between automatic library scans; 0 disables. */
  scan_interval_minutes: number;
  stream_buffer_seconds: number;
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
  /** Title art (a transparent logo of the title). */
  has_logo?: boolean;
  duration_ms: number | null;
  position_ms: number | null;
  played: boolean | null;
  rating: number | null;
  /** When it was added to the library (Unix ms). */
  added_at?: number;
}

/** `GET /api/home/featured`: titles for a home page hero. */
export interface FeaturedItem extends ItemCard {
  overview: string | null;
  tagline: string | null;
  official_rating: string | null;
  runtime_minutes: number | null;
  genres: string[];
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
  bitrate?: number | null;
  profile?: string | null;
  sample_rate?: number | null;
  channel_layout?: string | null;
}

export interface SubtitleStream {
  index: number;
  codec: string;
  language: string | null;
  title: string | null;
  is_text: boolean;
}

export interface VideoStream {
  index?: number;
  codec: string;
  profile?: string | null;
  width: number;
  height: number;
  bit_depth: number;
  pix_fmt?: string | null;
  hdr: "sdr" | "hdr10" | "hlg" | "dolby_vision";
  dv_profile: number | null;
  dv_compat_id?: number | null;
  bitrate?: number | null;
  frame_rate?: number | null;
  /** Colour tags; missing for files scanned before Kinora recorded them. */
  color_space?: string | null;
  color_primaries?: string | null;
  color_transfer?: string | null;
  color_range?: string | null;
}

export interface MediaInfo {
  container: string;
  duration_ms: number;
  bitrate: number | null;
  video: VideoStream | null;
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
  /** The same theme in the other scheme; paired themes follow light/dark mode. */
  pair?: string | null;
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
  /** Packaging of the HLS segments: MPEG-TS, or fragmented MP4 when HEVC is copied. */
  segments?: "mpeg_ts" | "fmp4";
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
  /** SDH / closed captions (the file's flag, or a title that says so). */
  is_hearing_impaired?: boolean;
  /** false for image subtitles (PGS, VobSub), which can't be shown in the browser yet. */
  supported: boolean;
}

/** `POST api/hls/{session}/keepalive`: the transcoder's live state. */
export interface SessionStatus {
  running: boolean;
  /** Paused on purpose: far enough ahead of the player. */
  throttled?: boolean;
  started_at_segment: number;
  last_requested_segment: number;
  transcoded_through: number | null;
  segment_seconds: number;
  hw: string;
  running_seconds: number;
  last_ffmpeg_line: string | null;
  /** Times realtime the current ffmpeg run is going (2 = twice as fast as playback). */
  speed?: number | null;
  /** Frames per second ffmpeg is making. */
  fps?: number | null;
}

export interface PlaybackStart {
  method: PlaybackPlan["method"];
  url: string;
  session_id: string | null;
  /** This playback on the admin dashboard (heartbeats go to api/now-playing/{play_id}). */
  play_id: string;
  plan: PlaybackPlan;
  duration_ms: number;
  subtitles: SubtitleTrack[];
  hardware: string;
  /** Tracks the server picked from this viewer's choices on other episodes (module "auto-tracks"). */
  auto_tracks?: AutoTracks;
  /** Seconds to buffer ahead (admin setting). Missing from older servers. */
  buffer_seconds?: number;
}

export interface AutoTracks {
  /** Which remembered choice was used. */
  from: "season" | "series" | "library";
  audio_index: number | null;
  /** Missing: no opinion. null: subtitles off. */
  subtitle_index?: number | null;
}

/** What the player reports about itself in its heartbeats (shown in the dashboard's Trace). */
export interface ClientStats {
  render_fps?: number | null;
  dropped_frames?: number | null;
  total_frames?: number | null;
  stalls?: number | null;
  stalled_ms?: number | null;
  buffered_ahead_s?: number | null;
  download_bps?: number | null;
  stream_bitrate_bps?: number | null;
  decoded_size?: string | null;
  player?: string | null;
  recoveries?: number | null;
  recent_errors?: string[];
}

export type PlayState = "playing" | "paused" | "buffering";

/** One live playback on the admin dashboard (`GET api/sessions`). */
export interface StreamInfo {
  id: string;
  method: PlaybackPlan["method"];
  state: PlayState;
  user: { id: string; username: string };
  client: { name: string | null; device: string | null; address: string | null };
  item: {
    id: string;
    kind: string;
    title: string;
    series_title: string | null;
    season: number | null;
    episode: number | null;
    year: number | null;
  };
  file_id: string;
  position_ms: number | null;
  duration_ms: number;
  started_at: number;
  last_seen_at: number;
  bitrate_bps: number | null;
  url: string;
  hardware: string;
  media: MediaInfo;
  plan: PlaybackPlan;
  transcoder: SessionStatus | null;
  client_stats: ClientStats | null;
}
