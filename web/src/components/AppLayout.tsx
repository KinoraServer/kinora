import { NamedIcon, libraryIcon } from "@/components/IconPicker";
import { Logo } from "@/components/Logo";
import { Home, LogOut, Palette, Search, Settings } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Link, NavLink, Outlet, useNavigate } from "react-router";
import { useApi, useSession } from "@/lib/session";
import { useTheme } from "@/lib/theme";
import type { Library } from "@/lib/types";
import { cn } from "@/lib/utils";

export function AppLayout() {
  const { user, isAdmin, logout, info } = useSession();
  const { data: libraries } = useApi<Library[]>("api/libraries");
  const navigate = useNavigate();
  const [query, setQuery] = useState("");

  const navClass = ({ isActive }: { isActive: boolean }) =>
    cn(
      "flex cursor-pointer items-center gap-2 rounded-md px-3 py-2 text-sm font-medium whitespace-nowrap transition",
      isActive ? "bg-accent text-accent-foreground" : "text-muted-foreground hover:text-foreground",
    );

  return (
    <div className="min-h-screen">
      <header className="sticky top-0 z-40 border-b bg-background/85 backdrop-blur">
        <div className="mx-auto flex h-16 max-w-[1800px] items-center gap-3 px-4 sm:px-8">
          <Link to="/" className="mr-2 flex cursor-pointer items-center gap-2 font-semibold">
            <Logo className="h-9 w-9" />
            <span className="hidden sm:inline">{info?.server_name ?? "Kinora"}</span>
          </Link>
          <nav className="scrollbar-none flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
            <NavLink to="/" end className={navClass}>
              <Home className="h-4 w-4" /> Home
            </NavLink>
            {libraries?.map((lib) => (
              <NavLink key={lib.id} to={`/library/${lib.id}`} className={navClass}>
                <NamedIcon name={libraryIcon(lib)} className="h-4 w-4" />
                {lib.name}
              </NavLink>
            ))}
          </nav>
          <form
            className="relative hidden md:block"
            onSubmit={(e) => {
              e.preventDefault();
              if (query.trim()) navigate(`/search?q=${encodeURIComponent(query.trim())}`);
            }}
          >
            <Search className="pointer-events-none absolute top-2.5 left-3 h-4 w-4 text-muted-foreground" />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search"
              aria-label="Search"
              className="h-9 w-56 rounded-md border bg-card pr-3 pl-9 text-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            />
          </form>
          <NavLink to="/search" className={cn(navClass({ isActive: false }), "md:hidden")} aria-label="Search">
            <Search className="h-4 w-4" />
          </NavLink>
          {isAdmin && (
            <NavLink to="/admin" className={navClass} aria-label="Admin">
              <Settings className="h-4 w-4" />
              <span className="hidden lg:inline">Admin</span>
            </NavLink>
          )}
          <UserMenu username={user?.username ?? ""} onLogout={logout} />
        </div>
      </header>
      <main className="mx-auto max-w-[1800px] px-4 py-6 sm:px-8">
        <Outlet />
      </main>
    </div>
  );
}

function UserMenu({ username, onLogout }: { username: string; onLogout: () => void }) {
  const [open, setOpen] = useState(false);
  const { themes, current, setTheme } = useTheme();
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const close = (e: MouseEvent) => ref.current && !ref.current.contains(e.target as Node) && setOpen(false);
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, []);

  return (
    <div className="relative" ref={ref}>
      <button
        onClick={() => setOpen((o) => !o)}
        className="flex h-9 w-9 cursor-pointer items-center justify-center rounded-full bg-accent text-sm font-semibold uppercase"
        aria-label="Account menu"
      >
        {username.slice(0, 1)}
      </button>
      {open && (
        <div className="absolute right-0 mt-2 w-60 rounded-lg border bg-card p-2 shadow-xl">
          <p className="px-2 py-1.5 text-sm font-medium">{username}</p>
          <div className="my-1 border-t" />
          <p className="flex items-center gap-2 px-2 py-1.5 text-xs font-medium tracking-wide text-muted-foreground uppercase">
            <Palette className="h-3.5 w-3.5" /> Theme
          </p>
          {themes.map((t) => (
            <button
              key={t.id}
              onClick={() => setTheme(t.id)}
              className={cn(
                "flex w-full cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm hover:bg-accent",
                current === t.id && "bg-accent",
              )}
            >
              <span
                className="h-4 w-4 rounded-full border"
                style={{
                  background: `linear-gradient(135deg, ${t.tokens.background ?? "#000"} 50%, ${t.tokens.primary ?? "#888"} 50%)`,
                }}
              />
              {t.name}
            </button>
          ))}
          <div className="my-1 border-t" />
          <button
            onClick={onLogout}
            className="flex w-full cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-accent"
          >
            <LogOut className="h-4 w-4" /> Sign out
          </button>
        </div>
      )}
    </div>
  );
}
