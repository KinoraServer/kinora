import {
  Archive, Baby, Bell, BookOpen, Bookmark, Bot, Brain, Brush, Camera, Car, ChefHat, Clapperboard, Cloud, Coffee,
  Compass, Cpu, Crown, Disc3, Drama, Dumbbell, Eye, Flag, Flame, Folder, Gamepad2, Ghost, Globe, GraduationCap,
  Headphones, Heart, House, Laugh, Leaf, Library, Medal, Mic, Moon, Mountain, Music, Newspaper, Palette, Plane,
  Podcast, Popcorn, Radio, Rocket, Shield, Skull, Smile, Snowflake, Sparkles, Star, Sun, Swords, Tag, Theater, Trophy,
  Tv, User, Users, Utensils, Video, Wine, Zap, ChevronDown, Search, X, Film, MonitorPlay, Projector, Ticket, type LucideIcon,
} from "lucide-react"; // prettier-ignore
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { cn } from "@/lib/utils";

/**
 * The icons people can choose from. This is a hand-picked list on purpose:
 * importing every lucide icon would add hundreds of KB to the app. To offer
 * another icon, import it above and add one line here. The key is the stable
 * name that gets saved, so never rename one.
 */
export const ICONS: Record<string, LucideIcon> = {
  // Movies and TV first
  film: Film, clapperboard: Clapperboard, popcorn: Popcorn, tv: Tv, "monitor-play": MonitorPlay, video: Video,
  projector: Projector, ticket: Ticket, camera: Camera, drama: Drama, theater: Theater, ghost: Ghost,
  skull: Skull, swords: Swords, rocket: Rocket, sparkles: Sparkles, laugh: Laugh, smile: Smile, heart: Heart,
  baby: Baby, "graduation-cap": GraduationCap, "book-open": BookOpen,
  // Music and audio
  music: Music, headphones: Headphones, mic: Mic, radio: Radio, "disc-3": Disc3, podcast: Podcast,
  // Everything else
  "gamepad-2": Gamepad2, star: Star, shield: Shield, crown: Crown, flame: Flame, zap: Zap, sun: Sun, moon: Moon,
  cloud: Cloud, snowflake: Snowflake, leaf: Leaf, mountain: Mountain, globe: Globe, compass: Compass, plane: Plane,
  car: Car, users: Users, user: User, library: Library, newspaper: Newspaper, trophy: Trophy, medal: Medal,
  dumbbell: Dumbbell, utensils: Utensils, "chef-hat": ChefHat, coffee: Coffee, wine: Wine, house: House,
  folder: Folder, archive: Archive, bookmark: Bookmark, tag: Tag, flag: Flag, bell: Bell, eye: Eye, brain: Brain,
  bot: Bot, cpu: Cpu, palette: Palette, brush: Brush,
}; // prettier-ignore

export const ICON_NAMES = Object.keys(ICONS);

/** "graduation-cap" -> "Graduation cap" */
export function iconLabel(name: string): string {
  const words = name.replace(/-\d+$/, "").split("-").join(" ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** Icons whose name contains every word in the search box. */
export function filterIcons(query: string, names: string[] = ICON_NAMES): string[] {
  const words = query
    .toLowerCase()
    .split(/[\s-]+/)
    .filter(Boolean);
  if (words.length === 0) return names;
  return names.filter((n) => words.every((w) => n.replace(/-/g, " ").includes(w)));
}

/** Renders a saved icon name. Unknown names (e.g. an icon removed later) fall back to `fallback`. */
export function NamedIcon({
  name,
  className,
  fallback = "film",
}: {
  name?: string | null;
  className?: string;
  fallback?: string;
}) {
  const Icon = (name && ICONS[name]) || ICONS[fallback] || Film;
  return <Icon className={className ?? "h-4 w-4"} aria-hidden="true" />;
}

interface IconPickerProps {
  /** Saved icon name, e.g. "film", or null for none. */
  value: string | null;
  onChange: (name: string | null) => void;
  /** Accessible name for the button. */
  label?: string;
  /** Show a "No icon" button. */
  clearable?: boolean;
  disabled?: boolean;
  className?: string;
}

/**
 * A button showing the current icon. Clicking opens a searchable grid of icons
 * to pick from.
 *
 *   <IconPicker value={icon} onChange={setIcon} />
 */
export function IconPicker({
  value,
  onChange,
  label = "Choose an icon",
  clearable,
  disabled,
  className,
}: IconPickerProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const rootRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const listId = useId();
  const shown = useMemo(() => filterIcons(query), [query]);

  useEffect(() => {
    if (!open) return;
    searchRef.current?.focus();
    const onPointer = (e: PointerEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    window.addEventListener("pointerdown", onPointer);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", onPointer);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const pick = (name: string | null) => {
    onChange(name);
    setOpen(false);
    setQuery("");
  };

  return (
    <div ref={rootRef} className={cn("relative inline-block", className)}>
      <button
        type="button"
        disabled={disabled}
        aria-label={label}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        onClick={() => setOpen((o) => !o)}
        className="flex h-10 cursor-pointer items-center gap-2 rounded-md border bg-background px-3 text-sm hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50 focus-visible:outline-2 focus-visible:outline-ring"
      >
        {value ? (
          <NamedIcon name={value} className="h-5 w-5" />
        ) : (
          <span className="h-5 w-5 rounded border border-dashed" />
        )}
        <span className="text-muted-foreground">{value ? iconLabel(value) : "No icon"}</span>
        <ChevronDown className="h-4 w-4 text-muted-foreground" />
      </button>

      {open && (
        <div className="absolute top-full left-0 z-50 mt-2 w-80 max-w-[calc(100vw-2rem)] rounded-xl border bg-card p-3 text-card-foreground shadow-xl">
          <div className="relative mb-3">
            <Search className="pointer-events-none absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
            <input
              ref={searchRef}
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search icons"
              aria-label="Search icons"
              className="h-9 w-full rounded-md border bg-background pr-8 pl-9 text-sm outline-none focus-visible:border-ring"
            />
            {query && (
              <button
                type="button"
                aria-label="Clear search"
                onClick={() => setQuery("")}
                className="absolute top-1/2 right-2 -translate-y-1/2 cursor-pointer rounded p-0.5 text-muted-foreground hover:text-foreground"
              >
                <X className="h-4 w-4" />
              </button>
            )}
          </div>
          {shown.length === 0 ? (
            <p className="py-6 text-center text-sm text-muted-foreground">No icons match "{query}".</p>
          ) : (
            <div
              id={listId}
              role="listbox"
              aria-label="Icons"
              className="grid max-h-64 grid-cols-6 gap-1 overflow-y-auto"
            >
              {shown.map((name) => (
                <button
                  key={name}
                  type="button"
                  role="option"
                  aria-selected={name === value}
                  title={iconLabel(name)}
                  aria-label={iconLabel(name)}
                  onClick={() => pick(name)}
                  className={cn(
                    "flex aspect-square cursor-pointer items-center justify-center rounded-md hover:bg-accent focus-visible:outline-2 focus-visible:outline-ring",
                    name === value && "bg-primary text-primary-foreground hover:bg-primary",
                  )}
                >
                  <NamedIcon name={name} className="h-5 w-5" />
                </button>
              ))}
            </div>
          )}
          {clearable && value && (
            <button
              type="button"
              onClick={() => pick(null)}
              className="mt-3 w-full cursor-pointer rounded-md border px-3 py-1.5 text-sm hover:bg-accent"
            >
              No icon
            </button>
          )}
        </div>
      )}
    </div>
  );
}

/** The icon a library shows: the one picked in its form, else a default for its type. */
export function libraryIcon(lib: { icon?: string | null; kind: string }): string {
  return lib.icon && ICONS[lib.icon] ? lib.icon : lib.kind === "tv" ? "tv" : "film";
}
