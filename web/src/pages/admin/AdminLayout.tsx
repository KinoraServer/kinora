import { useTranslation } from "react-i18next";
import { NavLink, Outlet } from "react-router";
import { cn } from "@/lib/utils";

const tabs: {
  to: string;
  labelKey:
    "admin.tab_dashboard" | "admin.tab_libraries" | "admin.tab_users" | "admin.tab_settings" | "admin.tab_modules";
  end?: boolean;
}[] = [
  { to: "/admin", labelKey: "admin.tab_dashboard", end: true },
  { to: "/admin/libraries", labelKey: "admin.tab_libraries" },
  { to: "/admin/users", labelKey: "admin.tab_users" },
  { to: "/admin/settings", labelKey: "admin.tab_settings" },
  { to: "/admin/modules", labelKey: "admin.tab_modules" },
];

export function AdminLayout() {
  const { t } = useTranslation();
  return (
    <div className="mx-auto max-w-5xl space-y-6">
      <h1 className="text-2xl font-semibold">{t("admin.title")}</h1>
      <nav className="scrollbar-none flex gap-1 overflow-x-auto border-b">
        {tabs.map((tab) => (
          <NavLink
            key={tab.to}
            to={tab.to}
            end={tab.end}
            className={({ isActive }) =>
              cn(
                "-mb-px cursor-pointer border-b-2 px-3 py-2 text-sm font-medium whitespace-nowrap",
                isActive
                  ? "border-primary text-foreground"
                  : "border-transparent text-muted-foreground hover:text-foreground",
              )
            }
          >
            {t(tab.labelKey)}
          </NavLink>
        ))}
      </nav>
      <Outlet />
    </div>
  );
}
