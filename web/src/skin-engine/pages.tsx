// Skinned routes: the shell and each page, rendered from the skin's
// templates. Loaded lazily, only when a skin is active (see routes.tsx).
//
// Every page has an error boundary: if a template throws, the viewer gets
// Kinora's own page for that view and the admin sees the error on the skin card.

import { Component, useContext, useEffect, useMemo, useRef, type ErrorInfo, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Outlet, useLocation, useNavigate, useParams, useSearchParams } from "react-router";
import { AppLayout, UserMenu } from "@/components/AppLayout";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";
import { DefaultPage, InSkinShell } from "./boundary";
import { pickTemplate } from "./compile";
import { attachBehaviours } from "./behaviours";
import { focusDefault } from "./focus";
import { nearest, SkinTemplate, type RenderEnv, type Scope, type SkinActions } from "./render";
import { useSkin } from "./SkinProvider";
import type { AppVM, CardVM, CompiledSkin, ItemVM, SkinNode } from "./types";
import { useAppVM, useHomeVM, useItemVM, useLibraryVM, useSearchVM } from "./viewmodels";
import { WatchPartyButton } from "@/watch-party/WatchPartyPanel";

// ---------------------------------------------------------------------------
// Error boundary
// ---------------------------------------------------------------------------

interface BoundaryProps {
  page: string;
  fallback: ReactNode;
  onError: (page: string, error: unknown) => void;
  children: ReactNode;
  resetKey: string;
}

class SkinBoundary extends Component<BoundaryProps, { failed: boolean; key: string }> {
  state = { failed: false, key: this.props.resetKey };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  static getDerivedStateFromProps(props: BoundaryProps, state: { failed: boolean; key: string }) {
    return props.resetKey !== state.key ? { failed: false, key: props.resetKey } : null;
  }
  componentDidCatch(error: unknown, _info: ErrorInfo) {
    this.props.onError(this.props.page, error);
  }
  render() {
    return this.state.failed ? this.props.fallback : this.props.children;
  }
}

// ---------------------------------------------------------------------------
// The skin root element
// ---------------------------------------------------------------------------

function optionAttributes(skin: CompiledSkin): { attrs: Record<string, string>; style: Record<string, string> } {
  const attrs: Record<string, string> = {};
  const style: Record<string, string> = {};
  for (const [k, v] of Object.entries(skin.bundle.optionValues)) {
    const key = k.replace(/[^a-z0-9-]/g, "");
    attrs[`data-opt-${key}`] = v;
    // Empty values are left unset so `var(--opt-x, fallback)` falls back.
    if (v !== "" && !/[;{}<>]/.test(v)) style[`--opt-${key}`] = v;
  }
  return { attrs, style };
}

function SkinRoot({ skin, app, children }: { skin: CompiledSkin; app: AppVM; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  const location = useLocation();
  const { device, input } = useSkin();
  useEffect(() => (ref.current ? attachBehaviours(ref.current) : undefined), []);
  // New page: start at the top, and on a TV put focus where the skin asks.
  useEffect(() => {
    const root = ref.current;
    if (!root || (input !== "remote" && device !== "tv")) return;
    // The page's data may still be loading: keep trying for a couple of seconds.
    let tries = 0;
    const t = window.setInterval(() => {
      if (focusDefault(root) || ++tries > 25) window.clearInterval(t);
    }, 80);
    return () => window.clearInterval(t);
  }, [location.pathname, input, device]);
  const { attrs, style } = useMemo(() => optionAttributes(skin), [skin]);
  return (
    <div
      ref={ref}
      data-app-skin={skin.bundle.id}
      data-device={device}
      data-input={input}
      data-scheme={skin.bundle.scheme ?? undefined}
      data-route={app.route.name}
      {...attrs}
      style={style}
    >
      {children}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Links and actions
// ---------------------------------------------------------------------------

function playPath(card: CardVM, page: unknown): string {
  if (card.isPlayable) {
    const t = card.$positionMs > 0 ? Math.floor(card.$positionMs / 1000) : 0;
    return `/play/${encodeURIComponent(card.id)}${t > 0 ? `?t=${t}` : ""}`;
  }
  const item = page as ItemVM | null;
  if (item && item.$type === "Item" && item.id === card.id && item.nextUp) return playPath(item.nextUp, null);
  return `/item/${encodeURIComponent(card.id)}`;
}

function useActions(page: unknown, reload: () => void): SkinActions {
  const navigate = useNavigate();
  const { logout } = useSession();
  const location = useLocation();
  const env = { page, app: null };
  return {
    link(kind, scope) {
      const card = () => nearest(scope, env as never, ["Card", "Item"]) as unknown as CardVM | null;
      const to = (path: string | null) => (path ? { href: `#${path}` } : null);
      switch (kind) {
        case "home":
          return to("/");
        case "search":
          return to("/search");
        case "settings":
          return to("/settings");
        case "admin":
          return to("/admin");
        case "back":
          return { href: "#", onClick: () => navigate(-1) };
        case "item": {
          const c = card();
          return to(c ? `/item/${encodeURIComponent(c.id)}` : null);
        }
        case "parent": {
          const c = card();
          return to(c?.parentId ? `/item/${encodeURIComponent(c.parentId)}` : null);
        }
        case "series": {
          const c = card();
          return to(c?.seriesId ? `/item/${encodeURIComponent(c.seriesId)}` : null);
        }
        case "library": {
          const l = nearest(scope, env as never, ["LibraryLink"]);
          return to(l ? `/library/${encodeURIComponent(String(l.id))}` : null);
        }
        case "season": {
          const s = nearest(scope, env as never, ["SeasonTab"]);
          const item = page as ItemVM | null;
          return to(
            s && item ? `/item/${encodeURIComponent(item.id)}?season=${encodeURIComponent(String(s.id))}` : null,
          );
        }
        case "play":
        case "resume": {
          const c = card();
          return to(c ? playPath(c, page) : null);
        }
        default:
          return null;
      }
    },
    run(action, scope: Scope) {
      const c = nearest(scope, env as never, ["Card", "Item"]) as unknown as CardVM | null;
      const item = page as ItemVM | null;
      switch (action) {
        case "play":
        case "resume":
          if (c) navigate(playPath(c, page));
          break;
        case "restart":
          if (c?.isPlayable) navigate(`/play/${encodeURIComponent(c.id)}`);
          else if (c && item?.nextUp && item.id === c.id) navigate(`/play/${encodeURIComponent(item.nextUp.id)}`);
          break;
        case "toggle-played":
          if (c) api.post(`api/items/${encodeURIComponent(c.id)}/played`, { played: !c.played }).then(reload, () => {});
          break;
        case "logout":
          logout();
          break;
        case "next-season":
        case "prev-season": {
          if (!item?.seasons?.length) break;
          const i = item.seasons.findIndex((s) => s.active);
          const next = item.seasons[i + (action === "next-season" ? 1 : -1)];
          if (next)
            navigate(`/item/${encodeURIComponent(item.id)}?season=${encodeURIComponent(next.id)}`, { replace: true });
          break;
        }
      }
    },
    select(kind, value) {
      if (kind === "sort") {
        const q = new URLSearchParams(location.search);
        q.set("sort", value);
        navigate(`${location.pathname}?${q}`, { replace: true });
      } else if (kind === "season") {
        const item = page as ItemVM | null;
        if (item)
          navigate(`/item/${encodeURIComponent(item.id)}?season=${encodeURIComponent(value)}`, { replace: true });
      }
    },
  };
}

/** Kinora widgets a skin can place with data-kn-component. */
function SearchBox() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  return (
    <form
      role="search"
      onSubmit={(e) => {
        e.preventDefault();
        const q = new FormData(e.currentTarget).get("q");
        if (typeof q === "string" && q.trim()) navigate(`/search?q=${encodeURIComponent(q.trim())}`);
      }}
    >
      <input
        name="q"
        type="search"
        placeholder={t("nav.search")}
        aria-label={t("nav.search")}
        className="h-9 w-56 rounded-md border bg-card px-3 text-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
      />
    </form>
  );
}

function useComponents(): Record<string, () => ReactNode> {
  const { user, isAdmin, logout } = useSession();
  return {
    "user-menu": () => (user ? <UserMenu user={user} isAdmin={isAdmin} onLogout={logout} /> : null),
    "search-box": () => <SearchBox />,
    "watch-party": () => <WatchPartyButton />,
  };
}

function useAppModel(skin: CompiledSkin) {
  const location = useLocation();
  const { device, input } = useSkin();
  const q = location.pathname === "/search" ? (new URLSearchParams(location.search).get("q") ?? "") : "";
  return useAppVM(location.pathname, q, skin.bundle.optionValues, device, input);
}

// ---------------------------------------------------------------------------
// Shell
// ---------------------------------------------------------------------------

export function SkinShell({ skin, nodes }: { skin: CompiledSkin; nodes: SkinNode[] }) {
  const app = useAppModel(skin);
  const location = useLocation();
  const actions = useActions(null, () => {});
  const components = useComponents();
  const { reportError } = useSkin();
  const env: RenderEnv = {
    skin,
    app,
    page: null,
    outlet: (
      <InSkinShell.Provider value={true}>
        <Outlet />
      </InSkinShell.Provider>
    ),
    actions,
    current: location.pathname + location.search,
    components,
  };
  return (
    <SkinBoundary page="shell" fallback={<AppLayout />} onError={reportError} resetKey={skin.bundle.hash}>
      <SkinRoot skin={skin} app={app}>
        <SkinTemplate nodes={nodes} env={env} root={app} />
      </SkinRoot>
    </SkinBoundary>
  );
}

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------

function PageView({
  skin,
  page,
  variant,
  vm,
  reload,
  fallback,
}: {
  skin: CompiledSkin;
  page: string;
  variant: string | null;
  vm: unknown;
  reload: () => void;
  fallback: ReactNode;
}) {
  const inShell = useContext(InSkinShell);
  const app = useAppModel(skin);
  const location = useLocation();
  const { device, reportError } = useSkin();
  const actions = useActions(vm, reload);
  const components = useComponents();
  const tpl = pickTemplate(skin.templates, page, variant, device);
  if (!tpl) return <DefaultPage>{fallback}</DefaultPage>;
  const env: RenderEnv = { skin, app, page: vm, actions, current: location.pathname + location.search, components };
  const content = <SkinTemplate nodes={tpl.nodes} env={env} root={vm} />;
  return (
    <SkinBoundary
      page={tpl.key}
      fallback={<DefaultPage>{fallback}</DefaultPage>}
      onError={reportError}
      resetKey={`${skin.bundle.hash}|${location.pathname}`}
    >
      {inShell ? (
        content
      ) : (
        <SkinRoot skin={skin} app={app}>
          {content}
        </SkinRoot>
      )}
    </SkinBoundary>
  );
}

/** Shown while a skinned page's data loads (keeps the layout from jumping). */
function Loading() {
  return <div aria-busy="true" style={{ minHeight: "60vh" }} />;
}

export function SkinnedHome({ skin, fallback }: { skin: CompiledSkin; fallback: ReactNode }) {
  const { vm, loading, error, reload } = useHomeVM();
  if (error) return <DefaultPage>{fallback}</DefaultPage>;
  if (loading) return <Loading />;
  return <PageView skin={skin} page="home" variant={null} vm={vm} reload={reload} fallback={fallback} />;
}

export function SkinnedLibrary({ skin, fallback }: { skin: CompiledSkin; fallback: ReactNode }) {
  const { id = "" } = useParams();
  const [params] = useSearchParams();
  const sort = params.get("sort") ?? "title";
  const { vm, loading, error, reload } = useLibraryVM(id, sort);
  if (error) return <DefaultPage>{fallback}</DefaultPage>;
  if (loading) return <Loading />;
  return <PageView skin={skin} page="library" variant={null} vm={vm} reload={reload} fallback={fallback} />;
}

export function SkinnedItem({ skin, fallback }: { skin: CompiledSkin; fallback: ReactNode }) {
  const { id = "" } = useParams();
  const [params] = useSearchParams();
  const { vm, loading, error, reload } = useItemVM(id, params.get("season"));
  if (error) return <DefaultPage>{fallback}</DefaultPage>;
  if (loading || !vm) return <Loading />;
  return <PageView skin={skin} page="item" variant={vm.kind} vm={vm} reload={reload} fallback={fallback} />;
}

export function SkinnedSearch({ skin, fallback }: { skin: CompiledSkin; fallback: ReactNode }) {
  const [params] = useSearchParams();
  const { vm, reload } = useSearchVM(params.get("q") ?? "");
  return <PageView skin={skin} page="search" variant={null} vm={vm} reload={reload} fallback={fallback} />;
}
