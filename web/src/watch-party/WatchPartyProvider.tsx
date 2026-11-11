import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { WatchPartyController, type PartyConfig } from "./controller";

interface WatchPartyValue {
  /** Null while the module is off (or its settings are loading). */
  controller: WatchPartyController | null;
  /** Fetch the module's settings again, e.g. after an admin changes them. */
  reload: () => void;
}

const WatchPartyContext = createContext<WatchPartyValue>({ controller: null, reload: () => {} });

/**
 * Holds the tab's watch party while the "watch-party" module is on. `navigate` opens a route; it comes from the
 * router outside React's tree, so the party survives page changes.
 */
export function WatchPartyProvider({ navigate, children }: { navigate: (path: string) => void; children: ReactNode }) {
  const { user } = useSession();
  const [config, setConfig] = useState<PartyConfig | null>(null);
  const [tick, setTick] = useState(0);
  const [controller, setController] = useState<WatchPartyController | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .get<PartyConfig>("api/watch-party")
      .then((c) => !cancelled && setConfig(c))
      .catch(() => !cancelled && setConfig(null));
    return () => {
      cancelled = true;
    };
  }, [tick]);

  const relay = config?.enabled ? (config.relay_url ?? "") : null;
  const client = config?.client;
  const name = user?.username ?? "";
  useEffect(() => {
    if (relay === null || !client) {
      setController(null);
      return;
    }
    let c: WatchPartyController;
    try {
      c = new WatchPartyController({ enabled: true, relay_url: relay || null, client }, name, navigate);
    } catch (e) {
      // A relay URL the SDK refuses: parties stay off rather than breaking the app.
      console.warn("watch party: can't start", e);
      setController(null);
      return;
    }
    setController(c);
    return () => {
      void c.dispose();
    };
  }, [relay, client, name, navigate]);

  const reload = useCallback(() => setTick((t) => t + 1), []);
  return <WatchPartyContext.Provider value={{ controller, reload }}>{children}</WatchPartyContext.Provider>;
}

/** The tab's watch party, re-rendering on every change; null while the module is off. */
export function useWatchParty(): WatchPartyController | null {
  const { controller } = useContext(WatchPartyContext);
  const subscribe = useCallback((fn: () => void) => (controller ? controller.subscribe(fn) : () => {}), [controller]);
  // The party's state is a new frozen object on every change, so it works as the snapshot, along with the
  // controller's own fields (toast, panel, prompts), which change together with a notification.
  useSyncExternalStore(subscribe, () => (controller ? snapshotOf(controller) : null));
  return controller;
}

export function useWatchPartyReload(): () => void {
  return useContext(WatchPartyContext).reload;
}

const snapshots = new WeakMap<WatchPartyController, { key: string; value: object }>();

/** A stable value that changes whenever anything the UI reads changes. */
function snapshotOf(c: WatchPartyController): object {
  const key = [c.toast?.id ?? 0, c.panelOpen, c.playBlocked, c.ended ?? "", c.playerItemId ?? "", c.playerPages].join(
    "|",
  );
  const prev = snapshots.get(c);
  if (prev && prev.key === key && (prev.value as { s: unknown }).s === c.state) return prev.value;
  const value = { s: c.state };
  snapshots.set(c, { key, value });
  return value;
}
