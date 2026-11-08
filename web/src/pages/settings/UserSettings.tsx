// The signed-in person's own settings: a list of sections (like a TV app's
// settings menu), each opening its own page. Profile and Display work today;
// Audio and Subtitles are placeholders for settings that come later.

import { AudioLines, Captions, ChevronLeft, ChevronRight, MonitorCog, User, type LucideIcon } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Link, Navigate, useParams } from "react-router";
import { Avatar } from "@/components/Avatar";
import { Button } from "@/components/ui/button";
import { Card, CardHeader, ErrorNote, Switch } from "@/components/ui/card";
import { Field, Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import { shrinkToAvatar } from "@/lib/avatar-image";
import { useApi, useSession } from "@/lib/session";
import type { User as UserType } from "@/lib/types";
import { useSkin } from "@/skin-engine/SkinProvider";

type SectionLabelKey =
  | "settings.sections.profile.label"
  | "settings.sections.display.label"
  | "settings.sections.audio.label"
  | "settings.sections.subtitles.label";

type SectionDescriptionKey =
  "settings.sections.profile.description" | "settings.sections.display.description" | "settings.coming_soon";

interface Section {
  id: string;
  labelKey: SectionLabelKey;
  descriptionKey: SectionDescriptionKey;
  icon: LucideIcon;
  page?: () => ReactNode;
}

export const SECTIONS: Section[] = [
  {
    id: "profile",
    labelKey: "settings.sections.profile.label",
    descriptionKey: "settings.sections.profile.description",
    icon: User,
    page: () => <ProfileSettings />,
  },
  {
    id: "display",
    labelKey: "settings.sections.display.label",
    descriptionKey: "settings.sections.display.description",
    icon: MonitorCog,
    page: () => <DisplaySettings />,
  },
  { id: "audio", labelKey: "settings.sections.audio.label", descriptionKey: "settings.coming_soon", icon: AudioLines },
  {
    id: "subtitles",
    labelKey: "settings.sections.subtitles.label",
    descriptionKey: "settings.coming_soon",
    icon: Captions,
  },
];

/** /settings: the list of sections. */
export function UserSettingsPage() {
  const { t } = useTranslation();
  const { user } = useSession();
  if (!user) return null;
  return (
    <div className="mx-auto max-w-2xl">
      <div className="mb-4 flex items-center gap-3 px-1">
        <Avatar user={user} className="h-10 w-10" />
        <h1 className="text-2xl font-semibold">{user.username}</h1>
      </div>
      <nav aria-label={t("settings.sections_aria")} className="space-y-0.5">
        {SECTIONS.map((s) => (
          <Link
            key={s.id}
            to={`/settings/${s.id}`}
            className="group flex cursor-pointer items-center gap-4 rounded-lg px-4 py-3 transition hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
          >
            <s.icon className="h-5 w-5 shrink-0 text-muted-foreground group-hover:text-foreground" aria-hidden />
            <span className="flex-1">
              <span className="block font-medium">{t(s.labelKey)}</span>
              <span className="block text-xs text-muted-foreground">{t(s.descriptionKey)}</span>
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
  const { t } = useTranslation();
  const { section } = useParams();
  const s = SECTIONS.find((x) => x.id === section);
  if (!s) return <Navigate to="/settings" replace />;
  const label = t(s.labelKey);
  return (
    <div className="mx-auto max-w-2xl space-y-6">
      <div>
        <Link
          to="/settings"
          className="-ml-1 inline-flex cursor-pointer items-center gap-1 text-sm text-muted-foreground hover:text-foreground"
        >
          <ChevronLeft className="h-4 w-4" /> {t("settings.back")}
        </Link>
        <h1 className="mt-1 flex items-center gap-2 text-2xl font-semibold">
          <s.icon className="h-6 w-6 text-muted-foreground" aria-hidden /> {label}
        </h1>
      </div>
      {s.page ? s.page() : <ComingSoon label={label} />}
    </div>
  );
}

function ComingSoon({ label }: { label: string }) {
  const { t } = useTranslation();
  return (
    <Card className="p-8 text-center">
      <p className="font-medium">{t("settings.coming_soon_title", { label })}</p>
      <p className="mt-1 text-sm text-muted-foreground">{t("settings.coming_soon_body")}</p>
    </Card>
  );
}

interface MySkin {
  allowed: boolean;
  server: string;
  mine: string | null;
  choices: { id: string; name: string; description: string | null; has_preview: boolean }[];
}

/** Display: which app skin to use (when the server allows picking) and safe mode for this browser. */
export function DisplaySettings() {
  const { t } = useTranslation();
  const { data, setData, error } = useApi<MySkin>("api/me/app-skin");
  const skin = useSkin();
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saved, flash] = useSavedFlash();
  const serverName = data?.choices.find((c) => c.id === data.server)?.name ?? t("settings.display.the_standard_look");

  const pick = async (value: string | null) => {
    setSaveError(null);
    try {
      await api.put("api/me/app-skin", { skin: value });
      if (data) setData({ ...data, mine: value });
      skin.reload();
      flash();
    } catch (e) {
      setSaveError((e as Error).message);
    }
  };

  const option = (value: string | null, label: string, description?: string | null) => (
    <label
      key={value ?? "follow"}
      className="flex cursor-pointer items-start gap-3 rounded-md px-2 py-2 hover:bg-accent"
    >
      <input
        type="radio"
        name="app-skin"
        className="mt-1 cursor-pointer accent-[var(--kn-primary)]"
        checked={(data?.mine ?? null) === value}
        onChange={() => pick(value)}
      />
      <span>
        <span className="block text-sm font-medium">{label}</span>
        {description && <span className="block text-xs text-muted-foreground">{description}</span>}
      </span>
    </label>
  );

  return (
    <div className="space-y-6">
      {data?.allowed && (
        <Card>
          <CardHeader title={t("settings.display.app_skin_title")} description={t("settings.display.app_skin_help")} />
          <div className="space-y-1 p-3" role="radiogroup" aria-label={t("settings.display.app_skin_title")}>
            {option(null, t("settings.display.same_as_server", { name: serverName }))}
            {option("none", t("skins.standard_look"))}
            {data.choices.map((c) => option(c.id, c.name, c.description))}
          </div>
          <div className="px-5 pb-4">
            <ErrorNote message={saveError} />
            {saved && <p className="text-sm text-muted-foreground">{t("common.saved")}</p>}
          </div>
        </Card>
      )}
      <ErrorNote message={error} />
      <Card>
        <div className="flex items-center justify-between gap-4 p-5">
          <div>
            <p className="font-medium">{t("skins.safe_mode")}</p>
            <p className="text-sm text-muted-foreground">{t("settings.display.safe_mode_help")}</p>
          </div>
          <Switch label={t("skins.safe_mode")} checked={skin.safeMode} onChange={skin.setSafeMode} />
        </div>
      </Card>
    </div>
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
  const { t } = useTranslation();
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
      <CardHeader title={t("settings.picture.title")} description={t("settings.picture.help")} />
      <div className="flex flex-wrap items-center gap-4 p-5">
        <Avatar user={user} className="h-20 w-20 text-3xl" />
        <div className="flex flex-wrap gap-2">
          <input
            ref={input}
            type="file"
            accept="image/png,image/jpeg,image/webp,image/gif"
            className="sr-only"
            aria-label={t("settings.picture.choose")}
            onChange={(e) => upload(e.target.files?.[0])}
          />
          <Button size="sm" onClick={() => input.current?.click()} disabled={busy}>
            {user.avatar_version ? t("settings.picture.change") : t("settings.picture.upload")}
          </Button>
          {user.avatar_version && (
            <Button size="sm" variant="outline" onClick={remove} disabled={busy}>
              {t("settings.picture.remove")}
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
  const { t } = useTranslation();
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
      <CardHeader title={t("settings.name.title")} />
      <form onSubmit={save} className="space-y-4 p-5">
        <Field label={t("settings.name.username")} help={t("settings.name.help")}>
          <Input value={name} maxLength={64} autoComplete="username" onChange={(e) => setName(e.target.value)} />
        </Field>
        <div className="flex items-center gap-3">
          <Button type="submit" size="sm" disabled={unchanged}>
            {t("common.save")}
          </Button>
          {saved && <span className="text-sm text-muted-foreground">{t("common.saved")}</span>}
        </div>
        <ErrorNote message={error} />
      </form>
    </Card>
  );
}

function PasswordCard() {
  const { t } = useTranslation();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [saved, flash] = useSavedFlash();

  const save = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    if (next.length < 8) return setError(t("settings.password.too_short"));
    if (next !== confirm) return setError(t("settings.password.mismatch"));
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
      <CardHeader title={t("settings.password.title")} description={t("settings.password.help")} />
      <form onSubmit={save} className="space-y-4 p-5">
        <Field label={t("settings.password.current")}>
          <Input
            type="password"
            autoComplete="current-password"
            value={current}
            onChange={(e) => setCurrent(e.target.value)}
          />
        </Field>
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label={t("settings.password.new")} help={t("settings.password.new_help")}>
            <Input type="password" autoComplete="new-password" value={next} onChange={(e) => setNext(e.target.value)} />
          </Field>
          <Field label={t("settings.password.confirm")}>
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
            {t("settings.password.change")}
          </Button>
          {saved && <span className="text-sm text-muted-foreground">{t("settings.password.changed")}</span>}
        </div>
        <ErrorNote message={error} />
      </form>
    </Card>
  );
}
