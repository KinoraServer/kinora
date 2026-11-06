import { NavLink, Outlet } from "react-router";
import { cn } from "@/lib/utils";

const tabs = [
  { to: "/admin", label: "Dashboard", end: true },
  { to: "/admin/libraries", label: "Libraries" },
  { to: "/admin/users", label: "Users" },
  { to: "/admin/settings", label: "Settings" },
  { to: "/admin/modules", label: "Modules & themes" },
];

export function AdminLayout() {
  return (
    <div className="mx-auto max-w-5xl space-y-6">
      <h1 className="text-2xl font-semibold">Admin</h1>
      <nav className="scrollbar-none flex gap-1 overflow-x-auto border-b">
        {tabs.map((t) => (
          <NavLink
            key={t.to}
            to={t.to}
            end={t.end}
            className={({ isActive }) =>
              cn(
                "-mb-px cursor-pointer border-b-2 px-3 py-2 text-sm font-medium whitespace-nowrap",
                isActive ? "border-primary text-foreground" : "border-transparent text-muted-foreground hover:text-foreground",
              )
            }
          >
            {t.label}
          </NavLink>
        ))}
      </nav>
      <Outlet />
    </div>
  );
}
