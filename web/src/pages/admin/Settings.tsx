import { Cpu, RefreshCw } from "lucide-react";
import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Badge, Card, CardHeader, ErrorNote, Spinner } from "@/components/ui/card";
import { Field, Input, Select } from "@/components/ui/input";
import { api } from "@/lib/api";
import { formatDate } from "@/lib/format";
import { useApi } from "@/lib/session";
import { useTheme } from "@/lib/theme";
import type { SystemSettings } from "@/lib/types";

interface Hardware {
  profile: { accel: { kind: string; device?: string | null }; tonemapper: string };
  ffmpeg_version: string | null;
  ffmpeg_found: boolean;
  tests: { name: string; ok: boolean; error: string | null }[];
  detected_at: number | null;
}

const ACCEL_LABEL: Record<
  string,
  | "admin.settings.accel_label_none"
  | "admin.settings.accel_label_nvenc"
  | "admin.settings.accel_label_qsv"
  | "admin.settings.accel_label_vaapi"
  | "admin.settings.accel_label_video_toolbox"
> = {
  none: "admin.settings.accel_label_none",
  nvenc: "admin.settings.accel_label_nvenc",
  qsv: "admin.settings.accel_label_qsv",
  vaapi: "admin.settings.accel_label_vaapi",
  video_toolbox: "admin.settings.accel_label_video_toolbox",
};

export function SettingsPage() {
  const { t } = useTranslation();
  const { data, error } = useApi<SystemSettings>("api/system/settings");
  const hardware = useApi<Hardware>("api/system/hardware");
  const { themes, reload: reloadThemes } = useTheme();
  const [form, setForm] = useState<SystemSettings | null>(null);
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
      setSaved(t("common.saved"));
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
        <CardHeader title={t("admin.settings.general")} />
        <ErrorNote message={error} />
        {form && (
          <form onSubmit={save} className="grid gap-4 p-5 sm:grid-cols-2">
            <Field label={t("admin.settings.server_name")}>
              <Input value={form.server_name} onChange={(e) => setForm({ ...form, server_name: e.target.value })} />
            </Field>
            <Field label={t("admin.settings.default_theme")} help={t("admin.settings.default_theme_help")}>
              <Select value={form.default_theme} onChange={(e) => setForm({ ...form, default_theme: e.target.value })}>
                {themes.map((th) => (
                  <option key={th.id} value={th.id}>
                    {th.name}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label={t("admin.settings.hw_accel")} help={t("admin.settings.hw_accel_help")}>
              <Select value={form.hw_accel} onChange={(e) => setForm({ ...form, hw_accel: e.target.value })}>
                <option value="auto">{t("admin.settings.accel_auto")}</option>
                <option value="nvenc">{t("admin.settings.accel_nvenc")}</option>
                <option value="qsv">{t("admin.settings.accel_qsv")}</option>
                <option value="vaapi">{t("admin.settings.accel_vaapi")}</option>
                <option value="videotoolbox">{t("admin.settings.accel_videotoolbox")}</option>
                <option value="none">{t("admin.settings.accel_none")}</option>
              </Select>
            </Field>
            <Field label={t("admin.settings.stream_buffer")} help={t("admin.settings.stream_buffer_help")}>
              <Input
                type="number"
                min={10}
                max={600}
                step={5}
                value={form.stream_buffer_seconds}
                onChange={(e) => setForm({ ...form, stream_buffer_seconds: Number(e.target.value) })}
              />
            </Field>
            <div className="flex items-center gap-3 sm:col-span-2">
              <Button type="submit">{t("common.save")}</Button>
              {saved && <span className="text-sm text-muted-foreground">{saved}</span>}
              <ErrorNote message={saveError} />
            </div>
          </form>
        )}
      </Card>

      <Card>
        <CardHeader
          title={t("admin.settings.hw_title")}
          description={t("admin.settings.hw_help")}
          action={
            <Button variant="secondary" size="sm" onClick={redetect}>
              <RefreshCw className="h-4 w-4" /> {t("admin.settings.detect")}
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
                  {ACCEL_LABEL[hardware.data.profile.accel.kind]
                    ? t(ACCEL_LABEL[hardware.data.profile.accel.kind])
                    : hardware.data.profile.accel.kind}
                </span>
                {hardware.data.profile.accel.device && <Badge>{hardware.data.profile.accel.device}</Badge>}
                <Badge>{t("admin.settings.tone_mapping", { value: hardware.data.profile.tonemapper })}</Badge>
              </div>
              {!hardware.data.ffmpeg_found && <ErrorNote message={t("admin.settings.ffmpeg_missing")} />}
              <p className="text-muted-foreground">
                {t("admin.settings.checked", {
                  version: hardware.data.ffmpeg_version,
                  when: formatDate(hardware.data.detected_at),
                })}
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
