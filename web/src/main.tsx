import { lazy, StrictMode, Suspense } from "react";
import { useTranslation } from "react-i18next";
import "@/i18n";
import { createRoot } from "react-dom/client";
import { createHashRouter, Navigate, RouterProvider } from "react-router";
import { Spinner } from "@/components/ui/card";
import { SessionProvider, useSession } from "@/lib/session";
import { ThemeProvider } from "@/lib/theme";
import { ActivityPage } from "@/pages/admin/Activity";
import { AdminLayout } from "@/pages/admin/AdminLayout";
import { DashboardPage } from "@/pages/admin/Dashboard";
import { LibrariesPage } from "@/pages/admin/Libraries";
import { ModulesPage } from "@/pages/admin/Modules";
import { SettingsPage } from "@/pages/admin/Settings";
import { UsersPage } from "@/pages/admin/Users";
import { LoginPage, SetupPage } from "@/pages/Auth";
import { HomePage } from "@/pages/Home";
import { ItemPage } from "@/pages/Item";
import { LibraryPage, SearchPage } from "@/pages/Library";
import { UserSettingsPage, UserSettingsSectionPage } from "@/pages/settings/UserSettings";
import { PreviewBanner } from "@/skin-engine/PreviewBanner";
import { DefaultPage, SkinLayout, SkinPage } from "@/skin-engine/routes";
import { SkinProvider } from "@/skin-engine/SkinProvider";
import "./index.css";

// The player pulls in hls.js, so it loads only when someone presses play.
const PlayerPage = lazy(() => import("@/pages/Player").then((m) => ({ default: m.PlayerPage })));

function AdminOnly({ children }: { children: React.ReactNode }) {
  const { isAdmin } = useSession();
  return isAdmin ? children : <Navigate to="/" replace />;
}

// Hash routing keeps every URL relative, so Kinora works behind a reverse
// proxy at any sub-path without configuration.
const router = createHashRouter([
  {
    path: "/play/:id",
    element: (
      <Suspense fallback={<div className="fixed inset-0 bg-black" />}>
        <PlayerPage />
      </Suspense>
    ),
  },
  {
    element: <SkinLayout />,
    children: [
      {
        index: true,
        element: (
          <SkinPage page="home">
            <HomePage />
          </SkinPage>
        ),
      },
      {
        path: "library/:id",
        element: (
          <SkinPage page="library">
            <LibraryPage />
          </SkinPage>
        ),
      },
      {
        path: "item/:id",
        element: (
          <SkinPage page="item">
            <ItemPage />
          </SkinPage>
        ),
      },
      {
        path: "search",
        element: (
          <SkinPage page="search">
            <SearchPage />
          </SkinPage>
        ),
      },
      {
        path: "settings",
        element: (
          <DefaultPage>
            <UserSettingsPage />
          </DefaultPage>
        ),
      },
      {
        path: "settings/:section",
        element: (
          <DefaultPage>
            <UserSettingsSectionPage />
          </DefaultPage>
        ),
      },
      {
        path: "admin",
        element: (
          <AdminOnly>
            <AdminLayout />
          </AdminOnly>
        ),
        children: [
          { index: true, element: <DashboardPage /> },
          { path: "activity", element: <ActivityPage /> },
          { path: "libraries", element: <LibrariesPage /> },
          { path: "users", element: <UsersPage /> },
          { path: "settings", element: <SettingsPage /> },
          { path: "modules", element: <ModulesPage /> },
        ],
      },
      { path: "*", element: <Navigate to="/" replace /> },
    ],
  },
]);

function Root() {
  const { t } = useTranslation();
  const { loading, info, user } = useSession();
  if (loading) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <Spinner className="h-8 w-8" />
      </div>
    );
  }
  if (!info) return <p className="p-8">{t("server.unreachable")}</p>;
  if (info.setup_required) return <SetupPage />;
  if (!user) return <LoginPage />;
  return (
    <SkinProvider>
      <RouterProvider router={router} />
      <PreviewBanner />
    </SkinProvider>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider>
      <SessionProvider>
        <Root />
      </SessionProvider>
    </ThemeProvider>
  </StrictMode>,
);
