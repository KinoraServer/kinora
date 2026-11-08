import { Logo } from "@/components/Logo";
import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Card, ErrorNote } from "@/components/ui/card";
import { Field, Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import { useSession } from "@/lib/session";

function AuthShell({ title, subtitle, children }: { title: string; subtitle: string; children: React.ReactNode }) {
  return (
    <div className="flex min-h-screen items-center justify-center p-4">
      <Card className="w-full max-w-sm p-8">
        <div className="mb-6 flex flex-col items-center gap-3 text-center">
          <Logo className="h-16 w-16" />
          <h1 className="text-xl font-semibold">{title}</h1>
          <p className="text-sm text-muted-foreground">{subtitle}</p>
        </div>
        {children}
      </Card>
    </div>
  );
}

function CredentialsForm({ endpoint, button, minLength }: { endpoint: string; button: string; minLength?: number }) {
  const { t } = useTranslation();
  const { refresh } = useSession();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.post(endpoint, { username, password, device: navigator.userAgent.slice(0, 120) });
      await refresh();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <form onSubmit={submit} className="space-y-4">
      <Field label={t("auth.username")}>
        <Input
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          autoComplete="username"
          required
          autoFocus
        />
      </Field>
      <Field
        label={t("auth.password")}
        help={minLength ? t("auth.password_min_length", { count: minLength }) : undefined}
      >
        <Input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoComplete={minLength ? "new-password" : "current-password"}
          minLength={minLength}
          required
        />
      </Field>
      <ErrorNote message={error} />
      <Button type="submit" className="w-full" disabled={busy}>
        {busy ? t("auth.waiting") : button}
      </Button>
    </form>
  );
}

export function SetupPage() {
  const { t } = useTranslation();
  return (
    <AuthShell title={t("auth.setup_title")} subtitle={t("auth.setup_subtitle")}>
      <CredentialsForm endpoint="api/setup" button={t("auth.create_account")} minLength={8} />
    </AuthShell>
  );
}

export function LoginPage() {
  const { t } = useTranslation();
  const { info } = useSession();
  return (
    <AuthShell title={info?.server_name ?? "Kinora"} subtitle={t("auth.login_subtitle")}>
      <CredentialsForm endpoint="api/auth/login" button={t("auth.sign_in")} />
    </AuthShell>
  );
}
