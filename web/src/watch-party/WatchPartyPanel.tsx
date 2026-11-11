import { Check, Copy, Crown, Popcorn, Search, UserMinus, X } from "lucide-react";
import { formatCode, type Participant } from "openwatchparty";
import { useState, type ReactNode } from "react";
import { Trans, useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Badge, ErrorNote, Spinner, Switch } from "@/components/ui/card";
import { Input, Select } from "@/components/ui/input";
import { api } from "@/lib/api";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import { errorOf, type WatchPartyController } from "./controller";
import { itemLabel, mediaLabel, playable, type PartyItem } from "./resolver";
import { useWatchParty } from "./WatchPartyProvider";

export const PRIVACY_URL = "https://openwatch.party/privacy";
export const SELF_HOSTING_URL = "https://openwatch.party/relay#self-hosting";

const link = "text-primary underline underline-offset-2 hover:brightness-110";

/** "+1.5 s", "−0.1 s". */
function seconds(ms: number): string {
  const s = Math.abs(ms) / 1000;
  const text = s >= 10 ? s.toFixed(0) : s.toFixed(1);
  return `${ms < 0 ? "−" : "+"}${text} s`;
}

/** The watch party side sheet: start or join a room, then everything about it. */
export function WatchPartyPanel({ className }: { className?: string }) {
  const { t } = useTranslation();
  const c = useWatchParty();
  if (!c || !c.panelOpen) return null;
  return (
    <aside
      aria-label={t("watch_party.title")}
      className={cn(
        "fixed inset-y-0 right-0 z-[70] flex w-full max-w-sm flex-col border-l bg-card text-card-foreground shadow-2xl",
        className,
      )}
      onKeyDown={(e) => {
        // The player's shortcuts (space, arrows, f) must not fire while typing or moving around here.
        e.stopPropagation();
        if (e.key === "Escape") c.setPanel(false);
      }}
    >
      <div className="flex items-center gap-2 border-b px-4 py-3">
        <Popcorn className="h-5 w-5 text-primary" />
        <h2 className="flex-1 font-semibold">{t("watch_party.title")}</h2>
        <Button
          variant="ghost"
          size="icon"
          className="h-8 w-8"
          aria-label={t("common.close")}
          onClick={() => c.setPanel(false)}
        >
          <X className="h-4 w-4" />
        </Button>
      </div>
      <div className="flex-1 space-y-5 overflow-y-auto p-4 text-sm">
        {c.active ? <InRoom c={c} /> : <OutOfRoom c={c} />}
        <RelayNote c={c} />
      </div>
    </aside>
  );
}

function OutOfRoom({ c }: { c: WatchPartyController }) {
  const { t } = useTranslation();
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notFound, setNotFound] = useState(false);

  const run = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    setNotFound(false);
    try {
      await fn();
    } catch (e) {
      setError(errorOf(e));
      setNotFound((e as { code?: string }).code === "room_not_found");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-5">
      {c.ended && <p className="rounded-md bg-muted px-3 py-2">{c.ended}</p>}
      <section className="space-y-2">
        <p className="text-muted-foreground">{t("watch_party.intro")}</p>
        <Button className="w-full" disabled={busy} onClick={() => run(() => c.create())}>
          {t("watch_party.start")}
        </Button>
        {c.playerItemId === null && <p className="text-xs text-muted-foreground">{t("watch_party.start_hint")}</p>}
      </section>
      <form
        className="space-y-2 border-t pt-4"
        onSubmit={(e) => {
          e.preventDefault();
          if (code.trim()) void run(() => c.join(code));
        }}
      >
        <label htmlFor="wp-code" className="block font-medium">
          {t("watch_party.join_label")}
        </label>
        <div className="flex gap-2">
          <Input
            id="wp-code"
            value={code}
            onChange={(e) => setCode(e.target.value)}
            placeholder="K7M-Q2X"
            autoComplete="off"
            autoCapitalize="characters"
            spellCheck={false}
            className="font-mono uppercase"
          />
          <Button type="submit" variant="secondary" disabled={busy || !code.trim()}>
            {t("watch_party.join")}
          </Button>
        </div>
      </form>
      {busy && <Spinner className="mx-auto" />}
      <ErrorNote message={error} />
      {notFound && (
        <Button variant="outline" className="w-full" disabled={busy} onClick={() => run(() => c.join(code, true))}>
          {t("watch_party.create_with_code")}
        </Button>
      )}
    </div>
  );
}

function InRoom({ c }: { c: WatchPartyController }) {
  const { t } = useTranslation();
  const s = c.state;
  const [copied, setCopied] = useState(false);
  const code = s.room ? formatCode(s.room.code) : null;

  return (
    <div className="space-y-5">
      {s.connection !== "connected" && (
        <p className="flex items-center gap-2 rounded-md bg-muted px-3 py-2">
          <Spinner className="h-4 w-4" />
          {s.connection === "reconnecting" ? t("watch_party.reconnecting") : t("watch_party.connecting")}
        </p>
      )}
      {code && (
        <section className="flex items-center justify-between gap-3 rounded-lg border p-3">
          <div>
            <p className="text-xs text-muted-foreground">{t("watch_party.code")}</p>
            <p className="font-mono text-2xl font-semibold tracking-widest">{code}</p>
          </div>
          <Button
            variant="outline"
            size="sm"
            onClick={() =>
              navigator.clipboard
                ?.writeText(code)
                .then(() => {
                  setCopied(true);
                  setTimeout(() => setCopied(false), 1500);
                })
                .catch(() => {})
            }
          >
            {copied ? <Check className="h-4 w-4" /> : <Copy className="h-4 w-4" />}
            {copied ? t("watch_party.copied") : t("watch_party.copy")}
          </Button>
        </section>
      )}

      <Media c={c} />
      {s.media && !s.isHost && <Offset c={c} />}
      <People c={c} />
      {s.isHost && <HostTools c={c} />}

      <Button variant="outline" className="w-full" onClick={() => c.leave()}>
        {t("watch_party.leave")}
      </Button>
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="space-y-2">
      <h3 className="text-xs font-semibold tracking-wide text-muted-foreground uppercase">{title}</h3>
      {children}
    </section>
  );
}

/** The room's title, this user's file for it, and what to do when they differ. */
function Media({ c }: { c: WatchPartyController }) {
  const { t } = useTranslation();
  const s = c.state;
  if (!s.media) {
    return (
      <Section title={t("watch_party.now_watching")}>
        <p className="text-muted-foreground">
          {s.isHost ? t("watch_party.host_play_hint") : t("watch_party.waiting_for_host")}
        </p>
      </Section>
    );
  }
  const own = s.choice?.id === "";
  const delta = s.choice ? s.candidates.find((r) => r.candidate.ref === s.choice?.ref)?.deltaMs : null;
  return (
    <Section title={t("watch_party.now_watching")}>
      {/* Titles from the room are plain text, never markup. */}
      <p className="font-medium">{mediaLabel(s.media)}</p>
      {!own && (
        <div className="space-y-2 rounded-lg border p-3">
          {s.match?.state === "searching" && (
            <p className="flex items-center gap-2 text-muted-foreground">
              <Spinner className="h-4 w-4" /> {t("watch_party.searching")}
            </p>
          )}
          {s.choice && (
            <div>
              <p className="text-xs text-muted-foreground">{t("watch_party.your_file")}</p>
              <p className="font-medium">{itemLabel(s.choice.ref)}</p>
              <p className="text-xs text-muted-foreground">
                {[
                  s.choice.ref.size_bytes ? formatBytes(s.choice.ref.size_bytes) : null,
                  delta != null && Math.abs(delta) >= 100
                    ? t("watch_party.length_diff", { diff: seconds(delta) })
                    : null,
                ]
                  .filter(Boolean)
                  .join(" · ")}
              </p>
            </div>
          )}
          {s.mismatch && (
            <div className="space-y-2">
              <p>
                {t("watch_party.mismatch", {
                  title: itemLabel(s.mismatch.candidate.ref),
                  diff: seconds(s.mismatch.deltaMs),
                })}
              </p>
              <div className="flex gap-2">
                <Button size="sm" onClick={() => c.party.acceptMismatch()}>
                  {t("watch_party.play_anyway")}
                </Button>
                <Button size="sm" variant="outline" onClick={() => c.party.declineMismatch()}>
                  {t("watch_party.sit_out")}
                </Button>
              </div>
            </div>
          )}
          {s.match?.state === "unavailable" && !s.choice && <p>{t("watch_party.not_found")}</p>}
          {c.canReturn && (
            <Button size="sm" className="w-full" onClick={() => c.returnToRoom()}>
              {t("watch_party.back_to_party")}
            </Button>
          )}
        </div>
      )}
      {!own && <ChooseList c={c} />}
      {own && c.canReturn && (
        <Button size="sm" className="w-full" onClick={() => c.returnToRoom()}>
          {t("watch_party.back_to_party")}
        </Button>
      )}
    </Section>
  );
}

/** The resolver's candidates and Kinora's own search, for a wrong or missing match. */
function ChooseList({ c }: { c: WatchPartyController }) {
  const { t } = useTranslation();
  const s = c.state;
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [found, setFound] = useState<PartyItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const kind = s.media?.kind === "episode" ? "episode" : "movie";

  const choose = (item: PartyItem) => {
    setError(null);
    void c.party.choose(playable(c.library.canonical(item))).then((ok) => {
      if (!ok) setError(t("watch_party.cant_choose"));
    });
  };

  const items = (found ?? s.candidates.map((r) => r.candidate.ref)).filter((i) => i !== s.choice?.ref);
  if (!open) {
    return (
      <button type="button" className={cn(link, "cursor-pointer text-sm")} onClick={() => setOpen(true)}>
        {s.choice ? t("watch_party.wrong_file") : t("watch_party.pick_file")}
      </button>
    );
  }
  return (
    <div className="space-y-2 rounded-lg border p-3">
      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          const q = new URLSearchParams({ q: query.trim(), kind });
          api
            .get<PartyItem[]>(`api/watch-party/search?${q}`)
            .then(setFound)
            .catch((err: Error) => setError(err.message));
        }}
      >
        <Input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={kind === "episode" ? t("watch_party.search_shows") : t("watch_party.search_movies")}
          aria-label={t("nav.search")}
          className="h-8"
        />
        <Button type="submit" size="sm" variant="secondary" aria-label={t("nav.search")} disabled={!query.trim()}>
          <Search className="h-4 w-4" />
        </Button>
      </form>
      {items.length === 0 ? (
        <p className="text-muted-foreground">{t("watch_party.no_candidates")}</p>
      ) : (
        <ul className="max-h-60 space-y-1 overflow-y-auto">
          {items.map((item) => {
            const delta = s.candidates.find((r) => r.candidate.ref === item)?.deltaMs;
            return (
              <li key={item.id}>
                <button
                  type="button"
                  onClick={() => choose(item)}
                  className="w-full cursor-pointer rounded-md px-2 py-1.5 text-left hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                >
                  <span className="block truncate">{itemLabel(item)}</span>
                  {delta != null && (
                    <span className="text-xs text-muted-foreground">
                      {t("watch_party.length_diff", { diff: seconds(delta) })}
                    </span>
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      )}
      <ErrorNote message={error} />
    </div>
  );
}

/** Local position = room position + offset, for a different cut of the title. */
function Offset({ c }: { c: WatchPartyController }) {
  const { t } = useTranslation();
  const s = c.state;
  const set = (ms: number) => {
    try {
      c.party.setOffset(Math.max(-604_800_000, Math.min(604_800_000, Math.round(ms))));
    } catch {
      // Outside a room or without media: nothing to shift.
    }
  };
  const hint = s.mismatch?.deltaMs ?? s.candidates.find((r) => r.candidate.ref === s.choice?.ref)?.deltaMs ?? null;
  const step = (ms: number, label: string) => (
    <Button size="sm" variant="outline" className="px-2 tabular-nums" onClick={() => set(s.offsetMs + ms)}>
      {label}
    </Button>
  );
  return (
    <Section title={t("watch_party.offset")}>
      <p className="text-muted-foreground">{t("watch_party.offset_help")}</p>
      <div className="flex flex-wrap items-center gap-1">
        {step(-1000, "−1 s")}
        {step(-100, "−0.1 s")}
        <span className="min-w-16 text-center font-mono tabular-nums">{seconds(s.offsetMs)}</span>
        {step(100, "+0.1 s")}
        {step(1000, "+1 s")}
        <Button size="sm" variant="ghost" onClick={() => set(0)} disabled={s.offsetMs === 0}>
          {t("watch_party.reset")}
        </Button>
      </div>
      {hint != null && Math.abs(hint) >= 100 && (
        <p className="text-xs text-muted-foreground">
          {t("watch_party.offset_hint", { diff: seconds(hint) })}{" "}
          <button type="button" className={cn(link, "cursor-pointer")} onClick={() => set(hint)}>
            {t("watch_party.use_it")}
          </button>
        </p>
      )}
    </Section>
  );
}

const PLAYER_KEYS = {
  idle: "watch_party.player.idle",
  loading: "watch_party.player.loading",
  ready: "watch_party.player.ready",
  buffering: "watch_party.player.buffering",
} as const;

const MATCH_KEYS = {
  searching: "watch_party.match.searching",
  matched: "watch_party.match.matched",
  mismatched: "watch_party.match.mismatched",
  unavailable: "watch_party.match.unavailable",
} as const;

function People({ c }: { c: WatchPartyController }) {
  const { t } = useTranslation();
  const s = c.state;
  const people = s.room?.participants ?? [];
  const status = (p: Participant) => {
    if (!p.connected) return t("watch_party.offline");
    if (p.match && p.match.state !== "matched") return t(MATCH_KEYS[p.match.state]);
    return t(PLAYER_KEYS[p.player]);
  };
  return (
    <Section title={t("watch_party.people", { count: people.length })}>
      <ul className="space-y-1">
        {people.map((p) => {
          const me = p.peerId === s.peerId;
          return (
            <li key={p.peerId} className="flex items-center gap-2 rounded-md px-1 py-1">
              <div className="min-w-0 flex-1">
                <p className="flex items-center gap-1.5 truncate font-medium">
                  {p.name}
                  {me && <span className="text-xs font-normal text-muted-foreground">{t("watch_party.you")}</span>}
                  {p.host && (
                    <Badge className="gap-1 px-1.5">
                      <Crown className="h-3 w-3" /> {t("watch_party.host")}
                    </Badge>
                  )}
                </p>
                <p className="truncate text-xs text-muted-foreground">
                  {status(p)}
                  {/* Other apps can join too: say which, as the relay reports it. */}
                  {!p.client.startsWith("Kinora/") && ` · ${p.client}`}
                </p>
              </div>
              {s.isHost && !me && (
                <>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-8 w-8"
                    aria-label={t("watch_party.make_host", { name: p.name })}
                    title={t("watch_party.make_host", { name: p.name })}
                    onClick={() => c.party.transferHost(p.peerId)}
                  >
                    <Crown className="h-4 w-4" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-8 w-8"
                    aria-label={t("watch_party.remove", { name: p.name })}
                    title={t("watch_party.remove", { name: p.name })}
                    onClick={() => c.party.kick(p.peerId)}
                  >
                    <UserMinus className="h-4 w-4" />
                  </Button>
                </>
              )}
            </li>
          );
        })}
      </ul>
    </Section>
  );
}

function HostTools({ c }: { c: WatchPartyController }) {
  const { t } = useTranslation();
  const room = c.state.room;
  if (!room) return null;
  return (
    <Section title={t("watch_party.host_tools")}>
      <label className="flex items-center justify-between gap-3">
        <span>{t("watch_party.control")}</span>
        <Select
          className="h-8 w-40"
          value={room.control}
          onChange={(e) => c.party.setControl(e.target.value === "everyone" ? "everyone" : "host")}
        >
          <option value="host">{t("watch_party.control_host")}</option>
          <option value="everyone">{t("watch_party.control_everyone")}</option>
        </Select>
      </label>
      <div className="flex items-center justify-between gap-3">
        <span>{t("watch_party.lock")}</span>
        <Switch checked={room.locked} onChange={(v) => c.party.lock(v)} label={t("watch_party.lock")} />
      </div>
      <div className="flex gap-2 pt-1">
        {c.state.media && (
          <Button size="sm" variant="outline" className="flex-1" onClick={() => c.party.clearMedia()}>
            {t("watch_party.stop_title")}
          </Button>
        )}
        <Button size="sm" variant="destructive" className="flex-1" onClick={() => c.party.end()}>
          {t("watch_party.end")}
        </Button>
      </div>
    </Section>
  );
}

/** Required by the public relay's terms: say it's in use, and link its privacy notice. */
function RelayNote({ c }: { c: WatchPartyController }) {
  const { t } = useTranslation();
  return (
    <p className="border-t pt-4 text-xs text-muted-foreground">
      {c.usesPublicRelay ? (
        <Trans
          i18nKey="watch_party.public_relay_note"
          components={{ privacy: <a href={PRIVACY_URL} target="_blank" rel="noreferrer" className={link} /> }}
        />
      ) : (
        t("watch_party.own_relay_note")
      )}
    </p>
  );
}

/** Short notes about what others did ("Ann paused"). */
export function WatchPartyToast() {
  const c = useWatchParty();
  if (!c?.toast) return null;
  return (
    <div
      role="status"
      key={c.toast.id}
      className="pointer-events-none fixed bottom-24 left-1/2 z-[70] -translate-x-1/2 rounded-full bg-black/85 px-4 py-2 text-sm text-white shadow-xl"
    >
      {c.toast.text}
    </div>
  );
}

/** The browser refused to start the video: playing has to come from a press. */
export function PlayBlockedPrompt() {
  const { t } = useTranslation();
  const c = useWatchParty();
  if (!c?.playBlocked) return null;
  return (
    <div className="absolute inset-0 z-[65] flex items-center justify-center bg-black/60">
      <div className="flex max-w-xs flex-col items-center gap-3 rounded-lg bg-black/85 p-5 text-center text-white">
        <p>{t("watch_party.play_blocked")}</p>
        <Button onClick={() => c.pressPlay()}>{t("watch_party.join_in")}</Button>
      </div>
    </div>
  );
}

/** Opens the panel; shows the room code while in a party. Nothing while the module is off. */
export function WatchPartyButton({ className }: { className?: string }) {
  const { t } = useTranslation();
  const c = useWatchParty();
  if (!c) return null;
  const code = c.active && c.state.room ? formatCode(c.state.room.code) : null;
  return (
    <button
      type="button"
      onClick={() => c.setPanel(!c.panelOpen)}
      aria-expanded={c.panelOpen}
      aria-label={code ? t("watch_party.button_in_room", { code }) : t("watch_party.title")}
      title={t("watch_party.title")}
      className={cn(
        "inline-flex h-9 cursor-pointer items-center gap-1.5 rounded-md px-2 text-sm font-medium hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
        code && "bg-primary/15 text-primary",
        className,
      )}
    >
      <Popcorn className="h-4 w-4" />
      {code && <span className="font-mono">{code}</span>}
    </button>
  );
}
