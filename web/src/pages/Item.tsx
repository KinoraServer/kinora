import { Check, ChevronLeft, Info, Play, RotateCcw, Star } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useNavigate, useParams } from "react-router";
import { Artwork } from "@/components/Artwork";
import { ItemGrid, ItemTile } from "@/components/ItemTile";
import { Button } from "@/components/ui/button";
import { Badge, ErrorNote, Spinner } from "@/components/ui/card";
import { api, imageUrl } from "@/lib/api";
import { episodeCode, formatBytes, formatDuration, progressPercent } from "@/lib/format";
import { useApi } from "@/lib/session";
import type { ItemDetail } from "@/lib/types";
import { cn } from "@/lib/utils";

export function ItemPage() {
  const { t } = useTranslation();
  const { id } = useParams();
  const navigate = useNavigate();
  const { data: item, error, loading, reload } = useApi<ItemDetail>(`api/items/${id}`, [id]);
  const [showFiles, setShowFiles] = useState(false);

  useEffect(() => setShowFiles(false), [id]);

  if (loading && !item) return <Spinner className="mx-auto mt-24" />;
  if (error || !item) return <ErrorNote message={error ?? t("item.not_found")} />;

  const playable = item.kind === "movie" || item.kind === "episode";
  const target = playable ? item : item.next_up;
  const duration = item.files[0]?.duration_ms ?? null;
  const resumeMs = playable && !item.played ? item.position_ms : 0;
  const file = item.files[0];

  const play = (fromStart = false) => {
    if (!target) return;
    const t = playable && !fromStart ? Math.floor(resumeMs / 1000) : 0;
    navigate(`/play/${target.id}${t > 0 ? `?t=${t}` : ""}`);
  };

  const togglePlayed = async () => {
    await api.post(`api/items/${item.id}/played`, { played: !item.played });
    reload();
  };

  const heading = item.kind === "episode" || item.kind === "season" ? item.series_title : null;

  return (
    <div className="-mt-6">
      {/* Backdrop */}
      <div className="relative -mx-4 h-[42vh] min-h-72 overflow-hidden sm:-mx-8">
        {item.has_backdrop && (
          <img
            src={imageUrl(item.id, "backdrop")}
            alt=""
            className="absolute inset-0 h-full w-full object-cover opacity-60"
          />
        )}
        <div className="absolute inset-0 bg-gradient-to-t from-background via-background/60 to-transparent" />
        <div className="absolute inset-0 bg-gradient-to-r from-background/90 to-transparent" />
        {item.parent_id && (
          <Link
            to={`/item/${item.parent_id}`}
            className="absolute top-4 left-4 flex cursor-pointer items-center gap-1 rounded-md bg-background/60 px-2 py-1 text-sm backdrop-blur sm:left-8"
          >
            <ChevronLeft className="h-4 w-4" /> {t("common.back")}
          </Link>
        )}
      </div>

      <div className="relative -mt-48 flex flex-col gap-8 md:flex-row">
        <Artwork
          id={item.id}
          kind={item.kind === "episode" ? "thumb" : "poster"}
          available={item.kind === "episode" ? item.has_thumb || item.has_backdrop : item.has_poster}
          title={item.title}
          className={cn(
            "shrink-0 rounded-xl shadow-2xl",
            item.kind === "episode" ? "aspect-video w-full max-w-md" : "aspect-[2/3] w-44 md:w-64",
          )}
        />
        <div className="min-w-0 flex-1 space-y-4 pt-4 md:pt-24">
          {heading && (
            <Link
              to={`/item/${item.series_id}`}
              className="cursor-pointer text-sm font-medium text-primary hover:underline"
            >
              {heading}
            </Link>
          )}
          <h1 className="text-3xl font-bold tracking-tight md:text-4xl">
            {item.kind === "episode" && (
              <span className="text-muted-foreground">{episodeCode(item.season_number, item.episode_number)} · </span>
            )}
            {item.title}
          </h1>
          {item.tagline && <p className="text-muted-foreground italic">{item.tagline}</p>}
          <div className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
            {item.year && <span>{item.year}</span>}
            {(item.runtime_minutes || duration) && (
              <span>{formatDuration((item.runtime_minutes ?? 0) * 60000 || duration)}</span>
            )}
            {item.rating && (
              <span className="flex items-center gap-1">
                <Star className="h-3.5 w-3.5 fill-current text-primary" /> {item.rating.toFixed(1)}
              </span>
            )}
            {file?.label && <Badge>{file.label}</Badge>}
            {item.genres.map((g) => (
              <Badge key={g}>{g}</Badge>
            ))}
          </div>

          <div className="flex flex-wrap gap-2">
            {target && (
              <Button size="lg" onClick={() => play()}>
                <Play className="h-5 w-5 fill-current" />
                {resumeMs > 0
                  ? t("item.resume")
                  : playable
                    ? t("item.play")
                    : t("item.play_episode", { code: episodeCode(target.season_number, target.episode_number) })}
              </Button>
            )}
            {resumeMs > 0 && (
              <Button size="lg" variant="secondary" onClick={() => play(true)}>
                <RotateCcw className="h-4 w-4" /> {t("item.from_start")}
              </Button>
            )}
            {item.kind !== "season" || item.children.length > 0 ? (
              <Button size="lg" variant="secondary" onClick={togglePlayed}>
                <Check className="h-4 w-4" /> {item.played ? t("item.mark_unwatched") : t("item.mark_watched")}
              </Button>
            ) : null}
            {file && (
              <Button
                size="lg"
                variant="ghost"
                onClick={() => setShowFiles((s) => !s)}
                aria-label={t("item.file_info")}
              >
                <Info className="h-4 w-4" />
              </Button>
            )}
          </div>

          {resumeMs > 0 && duration && (
            <div className="h-1 max-w-sm overflow-hidden rounded-full bg-muted">
              <div className="h-full bg-primary" style={{ width: `${progressPercent(resumeMs, duration)}%` }} />
            </div>
          )}

          {item.overview && <p className="max-w-3xl leading-relaxed text-foreground/90">{item.overview}</p>}

          {showFiles &&
            item.files.map((f) => (
              <div key={f.id} className="max-w-3xl space-y-1 rounded-lg border bg-card p-4 text-sm">
                <p className="font-mono text-xs break-all text-muted-foreground">{f.path}</p>
                <p>
                  {f.info?.container} · {formatBytes(f.size)}
                  {f.info?.bitrate ? ` · ${(f.info.bitrate / 1_000_000).toFixed(1)} Mbps` : ""}
                </p>
                {f.info?.video && (
                  <p>
                    {t("item.video_info", {
                      details:
                        `${f.info.video.codec} ${f.info.video.width}×${f.info.video.height} ${f.info.video.bit_depth}-bit` +
                        (f.info.video.hdr !== "sdr" && f.info.video.hdr ? ` ${f.info.video.hdr}` : "") +
                        (f.info.video.dv_profile ? ` profile ${f.info.video.dv_profile}` : ""),
                    })}
                  </p>
                )}
                {f.info?.audio.map((a) => (
                  <p key={a.index}>
                    {t("item.audio_info", {
                      details: [a.codec, `${a.channels}ch`, a.language, a.title].filter(Boolean).join(" "),
                    })}
                  </p>
                ))}
                {f.info?.subtitles.map((s) => (
                  <p key={s.index}>
                    {t("item.subtitle_info", { details: [s.codec, s.language, s.title].filter(Boolean).join(" ") })}
                  </p>
                ))}
              </div>
            ))}
        </div>
      </div>

      {item.kind === "series" && item.children.length > 0 && (
        <section className="mt-12 space-y-4">
          <h2 className="text-xl font-semibold">{t("item.seasons")}</h2>
          <ItemGrid items={item.children} />
        </section>
      )}
      {item.kind === "season" && (
        <section className="mt-12 space-y-4">
          <h2 className="text-xl font-semibold">{t("item.episodes")}</h2>
          <div className="grid grid-cols-[repeat(auto-fill,minmax(16rem,1fr))] gap-x-4 gap-y-6">
            {item.children.map((ep) => (
              <ItemTile key={ep.id} item={ep} />
            ))}
          </div>
        </section>
      )}
    </div>
  );
}
