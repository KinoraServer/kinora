// Where the app's routes meet the skin. With no skin active these render
// Kinora's own layout and pages and nothing else is loaded; the skinned
// versions are fetched only when a skin has templates for them.

import { lazy, Suspense, type ComponentType, type ReactNode } from "react";
import { useLocation } from "react-router";
import { AppLayout } from "@/components/AppLayout";
import { DefaultPage } from "./boundary";
import { pickTemplate } from "./compile";
import { useSkin } from "./SkinProvider";
import type { CompiledSkin } from "./types";

const load = () => import("./pages");
const SkinShell = lazy(() => load().then((m) => ({ default: m.SkinShell })));

type SkinnedPage = ComponentType<{ skin: CompiledSkin; fallback: ReactNode }>;
const PAGES: Record<"home" | "library" | "item" | "search", SkinnedPage> = {
  home: lazy(() => load().then((m) => ({ default: m.SkinnedHome }))),
  library: lazy(() => load().then((m) => ({ default: m.SkinnedLibrary }))),
  item: lazy(() => load().then((m) => ({ default: m.SkinnedItem }))),
  search: lazy(() => load().then((m) => ({ default: m.SkinnedSearch }))),
};

/** Does the skin have any template for this page (any variant or device)? */
export function hasPage(skin: CompiledSkin, page: string): boolean {
  for (const key of skin.templates.keys()) {
    if (key === `pages/${page}` || key.startsWith(`pages/${page}.`)) return true;
  }
  return false;
}

/** The app's layout: the skin's shell when it has one, Kinora's otherwise. Admin always uses Kinora's. */
export function SkinLayout() {
  const { skin } = useSkin();
  const { pathname } = useLocation();
  const shell = skin && !pathname.startsWith("/admin") ? skin.templates.get("shell") : undefined;
  if (!skin || !shell) return <AppLayout />;
  return (
    <Suspense fallback={<div className="min-h-screen" />}>
      <SkinShell skin={skin} nodes={shell} />
    </Suspense>
  );
}

/** One page: the skin's version when it has one, Kinora's (fenced off from the skin's CSS) otherwise. */
export function SkinPage({ page, children }: { page: keyof typeof PAGES; children: ReactNode }) {
  const { skin, device } = useSkin();
  if (!skin || !hasPage(skin, page) || (page !== "item" && !pickTemplate(skin.templates, page, null, device))) {
    return <DefaultPage>{children}</DefaultPage>;
  }
  const Page = PAGES[page];
  return (
    <Suspense fallback={<div aria-busy="true" className="min-h-[60vh]" />}>
      <Page skin={skin} fallback={children} />
    </Suspense>
  );
}

export { DefaultPage };
