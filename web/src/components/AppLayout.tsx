import { Avatar } from "@/components/Avatar";
import { Logo } from "@/components/Logo";
import { Clapperboard, Home, LogOut, Search, Settings, Tv, UserCog } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Link, NavLink, Outlet, useNavigate } from "react-router";
import { useApi, useSession } from "@/lib/session";
import type { Library, User } from "@/lib/types";
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
                {lib.kind === "tv" ? <Tv className="h-4 w-4" /> : <Clapperboard className="h-4 w-4" />}
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
          {user && <UserMenu user={user} isAdmin={isAdmin} onLogout={logout} />}
        </div>
      </header>
      <main className="mx-auto max-w-[1800px] px-4 py-6 sm:px-8">
        <Outlet />
      </main>
    </div>
  );
}

function UserMenu({ user, isAdmin, onLogout }: { user: User; isAdmin: boolean; onLogout: () => void }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const close = (e: MouseEvent) => ref.current && !ref.current.contains(e.target as Node) && setOpen(false);
    const escape = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", escape);
    };
  }, []);

  const item = "flex w-full cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm hover:bg-accent";

  return (
    <div className="relative" ref={ref}>
      <button
        onClick={() => setOpen((o) => !o)}
        className="cursor-pointer rounded-full focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        aria-label="Account menu"
        aria-expanded={open}
      >
        <Avatar user={user} />
      </button>
      {open && (
        <div className="absolute right-0 mt-2 w-56 rounded-lg border bg-card p-2 shadow-xl">
          <div className="flex items-center gap-2 px-2 py-1.5">
            <Avatar user={user} className="h-7 w-7 text-xs" />
            <p className="min-w-0 truncate text-sm font-medium">{user.username}</p>
          </div>
          <div className="my-1 border-t" />
          <Link to="/settings" className={item} onClick={() => setOpen(false)}>
            <UserCog className="h-4 w-4" /> User Settings
          </Link>
          {isAdmin && (
            <Link to="/admin" className={item} onClick={() => setOpen(false)}>
              <Settings className="h-4 w-4" /> Admin Settings
            </Link>
          )}
          <div className="my-1 border-t" />
          <button onClick={onLogout} className={item}>
            <LogOut className="h-4 w-4" /> Sign out
          </button>
        </div>
      )}
    </div>
  );
}
