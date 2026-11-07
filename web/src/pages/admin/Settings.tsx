import { Cpu, RefreshCw } from "lucide-react";
import { useEffect, useState, type FormEvent } from "react";
import { Button } from "@/components/ui/button";
import { Badge, Card, CardHeader, ErrorNote, Spinner } from "@/components/ui/card";
import { Field, Input, Select } from "@/components/ui/input";
import { api } from "@/lib/api";
import { formatDate } from "@/lib/format";
import { useApi } from "@/lib/session";
import { useTheme } from "@/lib/theme";

interface Settings {
  server_name: string;
  default_theme: string;
  hw_accel: string;
  scan_interval_minutes: number;
}

interface Hardware {
  profile: { accel: { kind: string; device?: string | null }; tonemapper: string };
  ffmpeg_version: string | null;
  ffmpeg_found: boolean;
  tests: { name: string; ok: boolean; error: string | null }[];
  detected_at: number | null;
}

const ACCEL_LABEL: Record<string, string> = {
  none: "Software (CPU)",
  nvenc: "NVIDIA NVENC",
  qsv: "Intel Quick Sync",
  vaapi: "VAAPI (Intel / AMD)",
  video_toolbox: "Apple VideoToolbox",
};

export function SettingsPage() {
  const { data, error } = useApi<Settings>("api/system/settings");
  const hardware = useApi<Hardware>("api/system/hardware");
  const { themes, reload: reloadThemes } = useTheme();
  const [form, setForm] = useState<Settings | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);

  useEffect(() => {
    if (data) setForm(data);
  }, [data]);

  const save = async (e: FormEvent) => {
    e.preventDefault();
    if (!form) return;
    setSaveError(null);
    try {
      await api.put("api/system/settings", form);
      setSaved("Saved.");
      reloadThemes();
      setTimeout(() => setSaved(null), 2000);
      if (data && data.hw_accel !== form.hw_accel) setTimeout(hardware.reload, 4000);
    } catch (err) {
      setSaveError((err as Error).message);
    }
  };

  const redetect = async () => {
    await api.post("api/system/hardware/detect");
    setTimeout(hardware.reload, 4000);
  };

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader title="General" />
        <ErrorNote message={error} />
        {form && (
          <form onSubmit={save} className="grid gap-4 p-5 sm:grid-cols-2">
            <Field label="Server name">
              <Input value={form.server_name} onChange={(e) => setForm({ ...form, server_name: e.target.value })} />
            </Field>
            <Field label="Default theme" help="Used by every browser that hasn't picked its own theme.">
              <Select value={form.default_theme} onChange={(e) => setForm({ ...form, default_theme: e.target.value })}>
                {themes.map((t) => (
                  <option key={t.id} value={t.id}>
                    {t.name}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label="Hardware acceleration" help="Auto tries each GPU backend and uses the first one that works.">
              <Select value={form.hw_accel} onChange={(e) => setForm({ ...form, hw_accel: e.target.value })}>
                <option value="auto">Auto-detect</option>
                <option value="nvenc">NVIDIA NVENC</option>
                <option value="qsv">Intel Quick Sync</option>
                <option value="vaapi">VAAPI (Intel / AMD on Linux)</option>
                <option value="videotoolbox">Apple VideoToolbox</option>
                <option value="none">Off (CPU only)</option>
              </Select>
            </Field>
            <Field label="Scan libraries every (minutes)" help="0 turns automatic scans off.">
              <Input
                type="number"
                min={0}
                value={form.scan_interval_minutes}
                onChange={(e) => setForm({ ...form, scan_interval_minutes: Number(e.target.value) })}
              />
            </Field>
            <div className="flex items-center gap-3 sm:col-span-2">
              <Button type="submit">Save</Button>
              {saved && <span className="text-sm text-muted-foreground">{saved}</span>}
              <ErrorNote message={saveError} />
            </div>
          </form>
        )}
      </Card>

      <Card>
        <CardHeader
          title="Hardware transcoding"
          description="What Kinora found on this machine."
          action={
            <Button variant="secondary" size="sm" onClick={redetect}>
              <RefreshCw className="h-4 w-4" /> Detect again
            </Button>
          }
        />
        <div className="space-y-4 p-5 text-sm">
          {hardware.loading && !hardware.data && <Spinner />}
          {hardware.data && (
            <>
              <div className="flex flex-wrap items-center gap-2">
                <Cpu className="h-5 w-5 text-primary" />
                <span className="font-medium">
                  {ACCEL_LABEL[hardware.data.profile.accel.kind] ?? hardware.data.profile.accel.kind}
                </span>
                {hardware.data.profile.accel.device && <Badge>{hardware.data.profile.accel.device}</Badge>}
                <Badge>Tone mapping: {hardware.data.profile.tonemapper}</Badge>
              </div>
              {!hardware.data.ffmpeg_found && <ErrorNote message="ffmpeg was not found, so only direct play works." />}
              <p className="text-muted-foreground">
                {hardware.data.ffmpeg_version} · checked {formatDate(hardware.data.detected_at)}
              </p>
              <ul className="space-y-1">
                {hardware.data.tests.map((t) => (
                  <li key={t.name} className="flex gap-2">
                    <span className={t.ok ? "text-primary" : "text-muted-foreground"}>{t.ok ? "✓" : "✗"}</span>
                    <span>
                      {t.name}
                      {t.error && <span className="block text-xs text-muted-foreground">{t.error}</span>}
                    </span>
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      </Card>
    </div>
  );
}
