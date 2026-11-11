// One watch party per tab, shared by every page. The party lives outside
// React so it survives moving between pages and players: the player page
// tells it when a video is open (`enterPlayer`) and when it closes
// (`leavePlayer`), and the controller attaches, detaches, shares the host's
// media and opens the player on the title the room matched.

import {
  attachVideo,
  checkMedia,
  PartyError,
  Relays,
  WatchParty,
  type PartyEvent,
  type PartyState,
} from "openwatchparty";
import { i18n } from "@/i18n";
import { KinoraLibrary } from "./library";
import { describe, libraryResolver, type PartyItem } from "./resolver";

/** `GET /api/watch-party`. */
export interface PartyConfig {
  enabled: boolean;
  /** The admin's own relay; null means the public relays. */
  relay_url: string | null;
  /** `Kinora/<version>`. */
  client: string;
}

export interface Toast {
  id: number;
  text: string;
}

interface Player {
  itemId: string;
  video: HTMLVideoElement;
  durationMs: number;
}

interface Attached {
  ref: PartyItem;
  video: HTMLVideoElement;
  detach: () => Promise<void>;
}

/** The user's own text for an SDK or relay error code; the error's message is for logs only. */
export function errorText(code: string): string {
  const known: Record<string, string> = {
    relay_full: i18n.t("watch_party.errors.relay_full"),
    unreachable: i18n.t("watch_party.errors.unreachable"),
    room_not_found: i18n.t("watch_party.errors.room_not_found"),
    room_full: i18n.t("watch_party.errors.room_full"),
    room_locked: i18n.t("watch_party.errors.room_locked"),
    invalid_code: i18n.t("watch_party.errors.invalid_code"),
    client_blocked: i18n.t("watch_party.errors.client_blocked"),
    forbidden: i18n.t("watch_party.errors.forbidden"),
    not_in_room: i18n.t("watch_party.errors.not_in_room"),
  };
  return known[code] ?? i18n.t("watch_party.errors.generic");
}

export function errorOf(e: unknown): string {
  if (e instanceof PartyError) {
    console.warn("watch party:", e.code, e.message);
    return errorText(e.code);
  }
  console.warn("watch party:", e);
  return i18n.t("watch_party.errors.generic");
}

/** A display name the relay accepts: 1 to 32 characters. */
export function displayName(name: string): string {
  const trimmed = Array.from(name.trim()).slice(0, 32).join("");
  return trimmed || "Kinora viewer";
}

export class WatchPartyController {
  readonly party: WatchParty<PartyItem>;
  readonly library = new KinoraLibrary();
  readonly usesPublicRelay: boolean;

  private player: Player | null = null;
  private attached: Attached | null = null;
  /** The choice the player was last opened for, so it opens once per choice. */
  private openedFor: PartyItem | null = null;
  /** The choice the user walked away from: don't open it again until they ask. */
  private dismissed: PartyItem | null = null;
  private sharing: string | null = null;
  private listeners = new Set<() => void>();
  private toastSeq = 0;
  private toastTimer: number | undefined;
  private unsubscribe: (() => void)[] = [];

  toast: Toast | null = null;
  /** The player refused to start: ask the user to press play. */
  playBlocked = false;
  /** Why the last party ended, until the user starts or joins another. */
  ended: string | null = null;
  /** Set while the panel should be shown. */
  panelOpen = false;
  /** Player pages open (they show the panel inside the player). */
  playerPages = 0;

  constructor(
    config: PartyConfig,
    name: string,
    private navigate: (path: string) => void,
  ) {
    this.usesPublicRelay = !config.relay_url;
    const relays = config.relay_url ? Relays.at(config.relay_url) : Relays.public();
    this.party = new WatchParty<PartyItem>({
      relays,
      client: config.client,
      name: displayName(name),
      resolver: libraryResolver(this.library),
    });
    // Subscribe before create or join: the party replays nothing.
    this.unsubscribe.push(
      this.party.on("state", () => {
        if (this.playBlocked && (this.party.state.player !== "loading" || this.party.state.room?.phase !== "playing")) {
          this.playBlocked = false;
        }
        this.sync();
        this.emit();
      }),
      this.party.on("event", (e) => this.onEvent(e)),
    );
  }

  get state(): PartyState<PartyItem> {
    return this.party.state;
  }

  /** In a room, or on the way into one. */
  get active(): boolean {
    const c = this.party.state.connection;
    return c === "connecting" || c === "connected" || c === "reconnecting";
  }

  /** The item the open player shows, if any. */
  get playerItemId(): string | null {
    return this.player?.itemId ?? null;
  }

  /** The room has a title this user walked away from. */
  get canReturn(): boolean {
    const ref = this.party.state.choice?.ref;
    return !!ref && this.player?.itemId !== ref.id;
  }

  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  private emit() {
    for (const fn of this.listeners) fn();
  }

  /** The player page is showing; returns the function that says it closed. */
  openPlayerPage(): () => void {
    this.playerPages++;
    this.emit();
    return () => {
      this.playerPages--;
      this.emit();
    };
  }

  setPanel(open: boolean) {
    this.panelOpen = open;
    this.emit();
  }

  private say(text: string) {
    this.toast = { id: ++this.toastSeq, text };
    window.clearTimeout(this.toastTimer);
    this.toastTimer = window.setTimeout(() => {
      this.toast = null;
      this.emit();
    }, 4000);
    this.emit();
  }

  private onEvent(event: PartyEvent) {
    switch (event.type) {
      case "action": {
        if (event.byMe) break;
        const who = event.by?.name ?? i18n.t("watch_party.the_room");
        const key = ACTION_KEYS[event.kind];
        if (key) this.say(i18n.t(key, { name: who }));
        break;
      }
      case "participantJoined":
        this.say(i18n.t("watch_party.joined", { name: event.participant.name }));
        break;
      case "participantLeft":
        this.say(i18n.t("watch_party.left", { name: event.participant.name }));
        break;
      case "playBlocked":
        this.playBlocked = true;
        this.emit();
        break;
      case "error":
        this.say(errorOf(event.error));
        break;
      case "resolveFailed":
        console.warn("watch party: matching failed", event.error);
        break;
      case "ended":
        // The party's end releases the player.
        this.attached = null;
        this.openedFor = null;
        this.dismissed = null;
        this.playBlocked = false;
        this.ended = i18n.t(END_KEYS[event.reason] ?? "watch_party.ended.lost");
        this.emit();
        break;
      default:
        break; // Event types a later SDK 1.x adds.
    }
  }

  // ---------------------------------------------------------------------------
  // Rooms
  // ---------------------------------------------------------------------------

  async create(): Promise<string> {
    this.ended = null;
    const code = await this.party.create();
    await this.shareIfHost();
    return code;
  }

  async join(code: string, create = false): Promise<void> {
    this.ended = null;
    await this.party.join(code, { create });
    await this.shareIfHost();
  }

  leave() {
    this.party.leave();
  }

  /** Back to the room's title after leaving the player. */
  returnToRoom() {
    const ref = this.party.state.choice?.ref;
    if (!ref) return;
    this.dismissed = null;
    this.openedFor = ref;
    this.navigate(`/play/${encodeURIComponent(ref.id)}`);
  }

  async dispose() {
    window.clearTimeout(this.toastTimer);
    for (const off of this.unsubscribe) off();
    this.listeners.clear();
    await this.party.close();
  }

  // ---------------------------------------------------------------------------
  // The player
  // ---------------------------------------------------------------------------

  /** The player page has `itemId` open in `video` and it shows a frame. */
  enterPlayer(itemId: string, video: HTMLVideoElement, durationMs: number) {
    this.player = { itemId, video, durationMs };
    const ref = this.party.state.choice?.ref;
    if (ref?.id === itemId) this.openedFor = ref;
    this.dismissed = null;
    void this.shareIfHost();
    this.sync();
    this.emit();
  }

  /**
   * The player page is closing. The room stops waiting for this user. Resolves once no SDK command runs on the
   * element, so the page can dispose of it.
   */
  leavePlayer(itemId: string): Promise<void> {
    if (this.player?.itemId !== itemId) return Promise.resolve();
    this.player = null;
    this.dismissed = this.party.state.choice?.ref ?? null;
    this.playBlocked = false;
    const done = this.detach();
    this.emit();
    return done;
  }

  /** Press play on the element itself, after the browser refused to start it (a user gesture). */
  pressPlay() {
    this.playBlocked = false;
    this.player?.video.play().catch(() => {});
    this.emit();
  }

  private detach(): Promise<void> {
    const a = this.attached;
    this.attached = null;
    return a ? a.detach() : this.party.detachPlayer();
  }

  /** Attach the open player to the room's choice, or open the player on it. */
  private sync() {
    const choice = this.party.state.choice?.ref ?? null;
    if (this.attached && (this.attached.ref !== choice || this.attached.video !== this.player?.video)) {
      void this.detach();
    }
    if (!choice || !this.active) return;
    if (this.player?.itemId === choice.id) {
      if (!this.attached) {
        // Claim the slot first: attaching changes the party's state, which calls sync() again right away.
        const video = this.player.video;
        const slot: Attached = { ref: choice, video, detach: () => this.party.detachPlayer() };
        this.attached = slot;
        const detach = attachVideo(this.party, video, choice);
        slot.detach = detach;
        if (this.attached !== slot) void detach();
      }
      return;
    }
    // A new title for the room: open it, once. A title the user walked away from stays closed.
    if (choice !== this.openedFor && choice !== this.dismissed) {
      this.openedFor = choice;
      this.navigate(`/play/${encodeURIComponent(choice.id)}`);
    }
  }

  /** As host, share what the player has open (once per title). */
  private async shareIfHost() {
    const player = this.player;
    const s = this.party.state;
    if (!player || !s.isHost || s.connection !== "connected") return;
    if (s.choice?.ref.id === player.itemId || this.sharing === player.itemId) return;
    this.sharing = player.itemId;
    try {
      const item = await this.library.item(player.itemId);
      if (this.player !== player || !this.party.state.isHost) return;
      const media = describe(item, item.duration_ms ?? player.durationMs);
      const problem = checkMedia(media);
      if (problem !== null) {
        console.warn("watch party: can't share this title:", problem);
        this.say(i18n.t("watch_party.cant_share"));
        return;
      }
      this.openedFor = item;
      this.party.setMedia(media, item, Math.floor(player.video.currentTime * 1000));
    } catch (e) {
      this.say(errorOf(e));
    } finally {
      this.sharing = null;
    }
  }
}

const ACTION_KEYS: Record<
  string,
  | "watch_party.actions.play"
  | "watch_party.actions.pause"
  | "watch_party.actions.seek"
  | "watch_party.actions.rate"
  | "watch_party.actions.media"
  | "watch_party.actions.ended"
  | "watch_party.actions.stall"
  | "watch_party.actions.host"
> = {
  play: "watch_party.actions.play",
  pause: "watch_party.actions.pause",
  seek: "watch_party.actions.seek",
  rate: "watch_party.actions.rate",
  media: "watch_party.actions.media",
  ended: "watch_party.actions.ended",
  stall: "watch_party.actions.stall",
  host: "watch_party.actions.host",
};

const END_KEYS: Record<
  string,
  | "watch_party.ended.left"
  | "watch_party.ended.ended"
  | "watch_party.ended.kicked"
  | "watch_party.ended.expired"
  | "watch_party.ended.replaced"
  | "watch_party.ended.lost"
> = {
  left: "watch_party.ended.left",
  ended: "watch_party.ended.ended",
  kicked: "watch_party.ended.kicked",
  expired: "watch_party.ended.expired",
  replaced: "watch_party.ended.replaced",
  lost: "watch_party.ended.lost",
};
