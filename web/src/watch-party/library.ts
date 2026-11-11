import type { Media } from "openwatchparty";
import { api } from "@/lib/api";
import type { PartyItem, PartyLibrary } from "./resolver";

function stopIfAborted(signal: AbortSignal) {
  if (signal.aborted) throw signal.reason ?? new DOMException("Aborted", "AbortError");
}

/**
 * The resolver's view of Kinora's library, through `/api/watch-party/*`.
 * The descriptor's values go to the server as JSON values and query parameters only.
 */
export class KinoraLibrary implements PartyLibrary {
  private items = new Map<string, PartyItem>();

  canonical(item: PartyItem): PartyItem {
    const known = this.items.get(item.id);
    if (known) return known;
    this.items.set(item.id, item);
    return item;
  }

  async item(id: string): Promise<PartyItem> {
    return this.canonical(await api.get<PartyItem>(`api/watch-party/items/${encodeURIComponent(id)}`));
  }

  async findByIds(media: Media, signal: AbortSignal): Promise<PartyItem[]> {
    const ids = media.ids ?? {};
    const seriesIds = media.kind === "episode" ? (media.series?.ids ?? {}) : {};
    if (Object.keys(ids).length === 0 && Object.keys(seriesIds).length === 0) return [];
    const found = await api.post<PartyItem[]>("api/watch-party/find", {
      ids,
      series_ids: seriesIds,
      season: media.season ?? null,
      episode: media.episode ?? null,
    });
    stopIfAborted(signal);
    return found;
  }

  async search(media: Media, signal: AbortSignal): Promise<PartyItem[]> {
    // An episode is searched by its show's title, never its own.
    const text = media.kind === "episode" ? (media.series?.title ?? media.title) : media.title;
    if (media.kind === "video" || !text.trim()) return [];
    const q = new URLSearchParams({ q: text.slice(0, 200), kind: media.kind });
    if (media.kind === "episode") {
      if (media.season !== undefined) q.set("season", String(media.season));
      if (media.episode !== undefined) q.set("episode", String(media.episode));
    }
    const found = await api.get<PartyItem[]>(`api/watch-party/search?${q}`);
    stopIfAborted(signal);
    return found;
  }
}
