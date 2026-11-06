import { Logo } from "@/components/Logo";
import { useState, type FormEvent } from "react";
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
      <Field label="Username">
        <Input
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          autoComplete="username"
          required
          autoFocus
        />
      </Field>
      <Field label="Password" help={minLength ? `At least ${minLength} characters.` : undefined}>
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
        {busy ? "Please wait..." : button}
      </Button>
    </form>
  );
}

export function SetupPage() {
  return (
    <AuthShell
      title="Welcome to Kinora"
      subtitle="Create the owner account for this server. You can add more people later."
    >
      <CredentialsForm endpoint="api/setup" button="Create account" minLength={8} />
    </AuthShell>
  );
}

export function LoginPage() {
  const { info } = useSession();
  return (
    <AuthShell title={info?.server_name ?? "Kinora"} subtitle="Sign in to continue">
      <CredentialsForm endpoint="api/auth/login" button="Sign in" />
    </AuthShell>
  );
}
