// Loads the viewer's app skin (or an admin's preview), compiles it once, and
// shares it with the pages. Nothing here runs when no skin is active, and the
// engine itself is only imported by the routes that need it.
//
// Kinora's own UI is used instead of a skin when:
//   - safe mode is on (`#/?skin=off`, or "Turn off the skin in this browser"),
//   - the browser can't scope CSS (no @scope support),
//   - the skin says it doesn't support this kind of device.

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { compileTemplates } from "./compile";
import { BASE_CSS, scopeAppSkinCss, supportsScope } from "./css";
import { installSpatialNavigation } from "./focus";
import type { CompiledSkin, Device, InputMode, SkinBundle } from "./types";

const SAFE_KEY = "kinora.skin-off";
const PREVIEW_KEY = "kinora.skin-preview";
const DEVICE_KEY = "kinora.skin-device";
const DEV_KEY = "kinora.skin-dev";
const STYLE_ID = "kn-app-skin-css";

const storage = {
  get(s: Storage | undefined, k: string): string | null {
    try {
      return s?.getItem(k) ?? null;
    } catch {
      return null;
    }
  },
  set(s: Storage | undefined, k: string, v: string | null) {
    try {
      if (v === null) s?.removeItem(k);
      else s?.setItem(k, v);
    } catch {
      // private mode: the setting just doesn't stick
    }
  },
};
const local = typeof localStorage !== "undefined" ? localStorage : undefined;
const session = typeof sessionStorage !== "undefined" ? sessionStorage : undefined;

const TV_UA =
  /SMART-TV|SmartTV|Tizen|Web0S|webOS|BRAVIA|Android TV|GoogleTV|AFT[A-Z]|CrKey|HbbTV|NetCast|Roku|Viera|AQUOS/i;
const DEVICES: Device[] = ["phone", "tablet", "desktop", "tv"];

/** What kind of screen this is. `#/?device=tv` (or Storybook) can force one for testing. */
export function detectDevice(): Device {
  const forced = storage.get(session, DEVICE_KEY);
  if (forced && (DEVICES as string[]).includes(forced)) return forced as Device;
  if (typeof window === "undefined") return "desktop";
  const w = window.innerWidth;
  const coarse = typeof matchMedia !== "undefined" && matchMedia("(pointer: coarse)").matches;
  if (TV_UA.test(navigator.userAgent) && w >= 960) return "tv";
  if (w < 640) return "phone";
  if (w < 1024 && coarse) return "tablet";
  return "desktop";
}

/** Read one-off switches from the URL: ?skin=off|on, ?device=tv, ?skin-dev=1. */
function readUrlSwitches() {
  if (typeof location === "undefined") return;
  const q = new URLSearchParams(location.hash.split("?")[1] ?? "");
  const skin = q.get("skin");
  if (skin === "off") storage.set(local, SAFE_KEY, "1");
  if (skin === "on") storage.set(local, SAFE_KEY, null);
  const device = q.get("device");
  if (device === "auto") storage.set(session, DEVICE_KEY, null);
  else if (device && (DEVICES as string[]).includes(device)) storage.set(session, DEVICE_KEY, device);
  if (q.get("skin-dev") === "1") storage.set(local, DEV_KEY, "1");
  if (q.get("skin-dev") === "0") storage.set(local, DEV_KEY, null);
}

export function compileSkin(bundle: SkinBundle): CompiledSkin {
  return {
    bundle,
    templates: compileTemplates(bundle.templates, bundle.id),
    css: scopeAppSkinCss(bundle.id, bundle.css),
  };
}

export interface SkinContextValue {
  /** The skin to render with, or null for Kinora's own UI. */
  skin: CompiledSkin | null;
  /** The loaded bundle even when it isn't used here (for the reason). */
  bundle: SkinBundle | null;
  /** Why a loaded skin isn't used on this device, if it isn't. */
  reason: string | null;
  device: Device;
  input: InputMode;
  setInput: (m: InputMode) => void;
  preview: string | null;
  startPreview: (id: string) => void;
  stopPreview: () => void;
  safeMode: boolean;
  setSafeMode: (on: boolean) => void;
  reload: () => void;
  reportError: (page: string, error: unknown) => void;
}

const noop = () => {};
const SkinContext = createContext<SkinContextValue>({
  skin: null,
  bundle: null,
  reason: null,
  device: "desktop",
  input: "pointer",
  setInput: noop,
  preview: null,
  startPreview: noop,
  stopPreview: noop,
  safeMode: false,
  setSafeMode: noop,
  reload: noop,
  reportError: noop,
});

export const useSkin = () => useContext(SkinContext);

const compiledCache = new Map<string, CompiledSkin>();

export function SkinProvider({ children }: { children: ReactNode }) {
  const { user, loading: sessionLoading } = useSession();
  const [bundle, setBundle] = useState<SkinBundle | null>(null);
  const [preview, setPreview] = useState<string | null>(() => storage.get(session, PREVIEW_KEY));
  const [safeMode, setSafe] = useState(() => {
    readUrlSwitches();
    return storage.get(local, SAFE_KEY) === "1";
  });
  const [device, setDevice] = useState<Device>(detectDevice);
  const [input, setInput] = useState<InputMode>(() => (detectDevice() === "tv" ? "remote" : "pointer"));
  const [tick, setTick] = useState(0);
  // Until the first answer, show nothing rather than flash the standard look.
  const [settled, setSettled] = useState(false);
  const etag = useRef<string | null>(null);

  // Load (and, while previewing or in skin-dev mode, keep checking for edits).
  useEffect(() => {
    if (!user || safeMode) {
      setBundle(null);
      if (!sessionLoading) setSettled(true);
      return;
    }
    let cancelled = false;
    const url = preview ? `api/app-skins/${encodeURIComponent(preview)}/bundle` : "api/app-skin/active";
    const load = async (first: boolean) => {
      try {
        const res = await fetch(url, {
          credentials: "same-origin",
          headers: !first && etag.current ? { "if-none-match": etag.current } : undefined,
        });
        if (cancelled || res.status === 304) return;
        if (res.status === 204 || !res.ok) {
          if (first || res.status === 204) setBundle(null);
          if (preview && res.status === 404) {
            storage.set(session, PREVIEW_KEY, null);
            setPreview(null);
          }
          return;
        }
        etag.current = res.headers.get("etag");
        const b = (await res.json()) as SkinBundle;
        if (!cancelled) setBundle(b);
      } catch {
        if (first && !cancelled) setBundle(null);
      }
    };
    etag.current = null;
    load(true).finally(() => !cancelled && setSettled(true));
    // Never hold the app back for long, even if the server is slow to answer.
    const giveUp = window.setTimeout(() => setSettled(true), 3000);
    const live = !!preview || storage.get(local, DEV_KEY) === "1";
    const timer = live ? window.setInterval(() => load(false), 2000) : undefined;
    return () => {
      cancelled = true;
      window.clearInterval(timer);
      window.clearTimeout(giveUp);
    };
  }, [user, sessionLoading, preview, safeMode, tick]);

  // The URL switches also work on a page that's already open.
  useEffect(() => {
    const onHash = () => {
      if (!/[?&](skin|device|skin-dev)=/.test(location.hash)) return;
      readUrlSwitches();
      setSafe(storage.get(local, SAFE_KEY) === "1");
      setDevice(detectDevice());
      setTick((t) => t + 1);
    };
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  // Device follows the window size; input follows what the viewer touches.
  useEffect(() => {
    const onResize = () => setDevice(detectDevice());
    const onPointer = (e: PointerEvent) => setInput(e.pointerType === "touch" ? "touch" : "pointer");
    window.addEventListener("resize", onResize);
    window.addEventListener("pointerdown", onPointer, { passive: true });
    window.addEventListener("pointermove", onPointer, { passive: true });
    return () => {
      window.removeEventListener("resize", onResize);
      window.removeEventListener("pointerdown", onPointer);
      window.removeEventListener("pointermove", onPointer);
    };
  }, []);

  const scopeOk = supportsScope();
  const deviceOk = !bundle || bundle.devices.length === 0 || bundle.devices.includes(device);
  const reason = !bundle
    ? null
    : !scopeOk
      ? "This browser is too old for app skins."
      : !deviceOk
        ? `This skin doesn't support ${device}s.`
        : null;

  const skin = useMemo(() => {
    if (!bundle || reason) return null;
    const key = `${bundle.id}:${bundle.hash}`;
    let c = compiledCache.get(key);
    if (!c) {
      c = compileSkin(bundle);
      compiledCache.set(key, c);
    }
    return c;
  }, [bundle, reason]);

  // The stylesheet: added while a skin is shown, removed otherwise.
  useEffect(() => {
    const existing = document.getElementById(STYLE_ID);
    if (!skin) {
      existing?.remove();
      return;
    }
    const el = existing ?? Object.assign(document.createElement("style"), { id: STYLE_ID });
    const text = `${BASE_CSS}\n${skin.css}`;
    if (el.textContent !== text) el.textContent = text;
    if (!existing) document.head.appendChild(el);
  }, [skin]);

  // Arrow keys and remote buttons, only while a skin is shown.
  useEffect(() => {
    if (!skin) return;
    return installSpatialNavigation(
      () => setInput("remote"),
      () => device === "tv",
    );
  }, [skin, device]);

  const reported = useRef(new Set<string>());
  const reportError = useCallback(
    (page: string, error: unknown) => {
      if (!bundle) return;
      const message = error instanceof Error ? error.message : String(error);
      const k = `${bundle.id}|${page}|${message}`;
      if (reported.current.has(k)) return;
      reported.current.add(k);
      console.warn(`[app skin ${bundle.id}] ${page} failed, showing the standard page instead:`, error);
      api.post("api/app-skins/errors", { skin: bundle.id, page, message }).catch(() => {});
    },
    [bundle],
  );

  const value: SkinContextValue = {
    skin,
    bundle,
    reason,
    device,
    input,
    setInput,
    preview,
    startPreview: (id) => {
      storage.set(session, PREVIEW_KEY, id);
      setPreview(id);
    },
    stopPreview: () => {
      storage.set(session, PREVIEW_KEY, null);
      setPreview(null);
    },
    safeMode,
    setSafeMode: (on) => {
      storage.set(local, SAFE_KEY, on ? "1" : null);
      setSafe(on);
    },
    reload: () => setTick((t) => t + 1),
    reportError,
  };
  return (
    <SkinContext.Provider value={value}>
      {settled ? children : <div className="min-h-screen" aria-busy="true" />}
    </SkinContext.Provider>
  );
}
