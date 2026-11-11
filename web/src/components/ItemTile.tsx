import { Check } from "lucide-react";
import type { Ref } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { episodeCode, progressPercent } from "@/lib/format";
import type { ItemCard } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Artwork } from "./Artwork";

/** Poster tile (movies, series, seasons) or wide thumbnail (episodes, continue watching). */
export function ItemTile({ item, wide = false, className }: { item: ItemCard; wide?: boolean; className?: string }) {
  const { t } = useTranslation();
  const isEpisode = item.kind === "episode";
  const useWide = wide || isEpisode;
  const progress = progressPercent(item.position_ms, item.duration_ms);
  const title = isEpisode && item.series_title ? item.series_title : item.title;
  const subtitle = isEpisode
    ? `${episodeCode(item.season_number, item.episode_number)} · ${item.title}`
    : item.year
      ? String(item.year)
      : item.kind === "series"
        ? t("item.series")
        : "";

  return (
    <Link to={`/item/${item.id}`} className={cn("group block cursor-pointer", className)}>
      <div className="relative">
        <Artwork
          id={item.id}
          kind={useWide ? (item.has_thumb ? "thumb" : "backdrop") : "poster"}
          available={useWide ? item.has_thumb || item.has_backdrop : item.has_poster}
          title={item.title}
          className={cn(
            "rounded-lg ring-primary transition group-hover:ring-2 group-focus-visible:ring-2",
            useWide ? "aspect-video" : "aspect-[2/3]",
          )}
        />
        {item.played && (
          <span className="absolute top-2 right-2 rounded-full bg-primary p-1 text-primary-foreground shadow">
            <Check className="h-3 w-3" />
          </span>
        )}
        {progress > 0 && !item.played && (
          <div className="absolute inset-x-2 bottom-2 h-1 overflow-hidden rounded-full bg-black/50">
            <div className="h-full bg-primary" style={{ width: `${progress}%` }} />
          </div>
        )}
      </div>
      <p className="mt-2 truncate text-sm font-medium">{title}</p>
      {subtitle && <p className="truncate text-xs text-muted-foreground">{subtitle}</p>}
    </Link>
  );
}

export function MediaRow({ title, items, wide }: { title: string; items: ItemCard[]; wide?: boolean }) {
  return (
    <section className="space-y-3">
      <h2 className="text-lg font-semibold">{title}</h2>
      {/* A scroller clips vertically too: the padding (pulled back with a negative margin)
          leaves room for the hover ring above the tiles. */}
      <div className="scrollbar-none -mx-4 -mt-1 flex gap-4 overflow-x-auto px-4 pt-1 pb-2 sm:-mx-8 sm:px-8">
        {items.map((item) => (
          <ItemTile
            key={item.id}
            item={item}
            wide={wide}
            className={wide || item.kind === "episode" ? "w-64 shrink-0" : "w-36 shrink-0 sm:w-40"}
          />
        ))}
      </div>
    </section>
  );
}

export function ItemGrid({ items, ref }: { items: ItemCard[]; ref?: Ref<HTMLDivElement> }) {
  return (
    <div
      ref={ref}
      className="grid grid-cols-[repeat(auto-fill,minmax(8.5rem,1fr))] gap-x-4 gap-y-6 sm:grid-cols-[repeat(auto-fill,minmax(10rem,1fr))]"
    >
      {items.map((item) => (
        <ItemTile key={item.id} item={item} />
      ))}
    </div>
  );
}
