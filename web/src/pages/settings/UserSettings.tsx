// The signed-in person's own settings: a list of sections (like a TV app's
// settings menu), each opening its own page. Profile works today; Display,
// Audio and Subtitles are placeholders for settings that come later.

import { AudioLines, Captions, ChevronLeft, ChevronRight, MonitorCog, User, type LucideIcon } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
import { Link, Navigate, useParams } from "react-router";
import { Avatar } from "@/components/Avatar";
import { Button } from "@/components/ui/button";
import { Card, CardHeader, ErrorNote } from "@/components/ui/card";
import { Field, Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import { shrinkToAvatar } from "@/lib/avatar-image";
import { useSession } from "@/lib/session";
import type { User as UserType } from "@/lib/types";

interface Section {
  id: string;
  label: string;
  description: string;
  icon: LucideIcon;
  page?: () => ReactNode;
}

export const SECTIONS: Section[] = [
  {
    id: "profile",
    label: "Profile",
    description: "Picture, name and password",
    icon: User,
    page: () => <ProfileSettings />,
  },
  { id: "display", label: "Display", description: "Coming soon", icon: MonitorCog },
  { id: "audio", label: "Audio", description: "Coming soon", icon: AudioLines },
  { id: "subtitles", label: "Subtitles", description: "Coming soon", icon: Captions },
];

/** /settings: the list of sections. */
export function UserSettingsPage() {
  const { user } = useSession();
  if (!user) return null;
  return (
    <div className="mx-auto max-w-2xl">
      <div className="mb-4 flex items-center gap-3 px-1">
        <Avatar user={user} className="h-10 w-10" />
        <h1 className="text-2xl font-semibold">{user.username}</h1>
      </div>
      <nav aria-label="Settings sections" className="space-y-0.5">
        {SECTIONS.map((s) => (
          <Link
            key={s.id}
            to={`/settings/${s.id}`}
            className="group flex cursor-pointer items-center gap-4 rounded-lg px-4 py-3 transition hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
          >
            <s.icon className="h-5 w-5 shrink-0 text-muted-foreground group-hover:text-foreground" aria-hidden />
            <span className="flex-1">
              <span className="block font-medium">{s.label}</span>
              <span className="block text-xs text-muted-foreground">{s.description}</span>
            </span>
            <ChevronRight className="h-4 w-4 text-muted-foreground opacity-0 transition group-hover:opacity-100" />
          </Link>
        ))}
      </nav>
    </div>
  );
}

/** /settings/:section */
export function UserSettingsSectionPage() {
  const { section } = useParams();
  const s = SECTIONS.find((x) => x.id === section);
  if (!s) return <Navigate to="/settings" replace />;
  return (
    <div className="mx-auto max-w-2xl space-y-6">
      <div>
        <Link
          to="/settings"
          className="-ml-1 inline-flex cursor-pointer items-center gap-1 text-sm text-muted-foreground hover:text-foreground"
        >
          <ChevronLeft className="h-4 w-4" /> Settings
        </Link>
        <h1 className="mt-1 flex items-center gap-2 text-2xl font-semibold">
          <s.icon className="h-6 w-6 text-muted-foreground" aria-hidden /> {s.label}
        </h1>
      </div>
      {s.page ? s.page() : <ComingSoon label={s.label} />}
    </div>
  );
}

function ComingSoon({ label }: { label: string }) {
  return (
    <Card className="p-8 text-center">
      <p className="font-medium">{label} settings are coming</p>
      <p className="mt-1 text-sm text-muted-foreground">Nothing to change here yet.</p>
    </Card>
  );
}

/** Shows "Saved." for two seconds after `flash()`. */
function useSavedFlash() {
  const [saved, setSaved] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  const flash = () => {
    setSaved(true);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setSaved(false), 2000);
  };
  return [saved, flash] as const;
}

export function ProfileSettings() {
  return (
    <div className="space-y-6">
      <PictureCard />
      <NameCard />
      <PasswordCard />
    </div>
  );
}

function PictureCard() {
  const { user, setUser } = useSession();
  const input = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (!user) return null;

  const upload = async (file: File | undefined) => {
    if (!file) return;
    setBusy(true);
    setError(null);
    try {
      setUser(await api.put<UserType>("api/auth/avatar", await shrinkToAvatar(file)));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
      if (input.current) input.current.value = "";
    }
  };

  const remove = async () => {
    setBusy(true);
    setError(null);
    try {
      setUser(await api.del<UserType>("api/auth/avatar"));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card>
      <CardHeader title="Profile picture" description="Shown in the top bar. Square pictures look best." />
      <div className="flex flex-wrap items-center gap-4 p-5">
        <Avatar user={user} className="h-20 w-20 text-3xl" />
        <div className="flex flex-wrap gap-2">
          <input
            ref={input}
            type="file"
            accept="image/png,image/jpeg,image/webp,image/gif"
            className="sr-only"
            aria-label="Choose a profile picture"
            onChange={(e) => upload(e.target.files?.[0])}
          />
          <Button size="sm" onClick={() => input.current?.click()} disabled={busy}>
            {user.avatar_version ? "Change picture" : "Upload picture"}
          </Button>
          {user.avatar_version && (
            <Button size="sm" variant="outline" onClick={remove} disabled={busy}>
              Remove
            </Button>
          )}
        </div>
        <div className="w-full empty:hidden">
          <ErrorNote message={error} />
        </div>
      </div>
    </Card>
  );
}

function NameCard() {
  const { user, setUser } = useSession();
  const [name, setName] = useState(user?.username ?? "");
  const [error, setError] = useState<string | null>(null);
  const [saved, flash] = useSavedFlash();
  if (!user) return null;

  const save = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      const next = await api.patch<UserType>("api/auth/profile", { username: name });
      setUser(next);
      setName(next.username);
      flash();
    } catch (err) {
      setError((err as Error).message);
    }
  };

  const unchanged = name.trim() === user.username || !name.trim();
  return (
    <Card>
      <CardHeader title="Name" />
      <form onSubmit={save} className="space-y-4 p-5">
        <Field label="Username" help="You sign in with this name.">
          <Input value={name} maxLength={64} autoComplete="username" onChange={(e) => setName(e.target.value)} />
        </Field>
        <div className="flex items-center gap-3">
          <Button type="submit" size="sm" disabled={unchanged}>
            Save
          </Button>
          {saved && <span className="text-sm text-muted-foreground">Saved.</span>}
        </div>
        <ErrorNote message={error} />
      </form>
    </Card>
  );
}

function PasswordCard() {
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [saved, flash] = useSavedFlash();

  const save = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    if (next.length < 8) return setError("The new password needs at least 8 characters.");
    if (next !== confirm) return setError("The new passwords don't match.");
    setBusy(true);
    try {
      await api.post("api/auth/password", { current_password: current, new_password: next });
      setCurrent("");
      setNext("");
      setConfirm("");
      flash();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card>
      <CardHeader title="Password" description="You stay signed in on this device." />
      <form onSubmit={save} className="space-y-4 p-5">
        <Field label="Current password">
          <Input
            type="password"
            autoComplete="current-password"
            value={current}
            onChange={(e) => setCurrent(e.target.value)}
          />
        </Field>
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="New password" help="At least 8 characters.">
            <Input type="password" autoComplete="new-password" value={next} onChange={(e) => setNext(e.target.value)} />
          </Field>
          <Field label="Confirm new password">
            <Input
              type="password"
              autoComplete="new-password"
              value={confirm}
              onChange={(e) => setConfirm(e.target.value)}
            />
          </Field>
        </div>
        <div className="flex items-center gap-3">
          <Button type="submit" size="sm" disabled={busy || !current || !next || !confirm}>
            Change password
          </Button>
          {saved && <span className="text-sm text-muted-foreground">Password changed.</span>}
        </div>
        <ErrorNote message={error} />
      </form>
    </Card>
  );
}
