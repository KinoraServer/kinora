import { Trash2, UserPlus } from "lucide-react";
import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Badge, Card, CardHeader, ErrorNote } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { Field, Input, Select } from "@/components/ui/input";
import { api } from "@/lib/api";
import { formatDate } from "@/lib/format";
import { useApi, useSession } from "@/lib/session";
import type { Role } from "@/lib/types";

interface UserRow {
  id: string;
  username: string;
  role: Role;
  created_at: number;
  last_login_at: number | null;
}

const ROLE_HELP: Record<
  Role,
  "admin.users.role_owner" | "admin.users.role_admin" | "admin.users.role_editor" | "admin.users.role_viewer"
> = {
  owner: "admin.users.role_owner",
  admin: "admin.users.role_admin",
  editor: "admin.users.role_editor",
  viewer: "admin.users.role_viewer",
};

export function UsersPage() {
  const { t } = useTranslation();
  const { user } = useSession();
  const { data, reload, error } = useApi<UserRow[]>("api/users");
  const [open, setOpen] = useState(false);

  const remove = async (u: UserRow) => {
    if (!confirm(t("admin.users.delete_confirm", { username: u.username }))) return;
    await api.del(`api/users/${u.id}`).catch((e: Error) => alert(e.message));
    reload();
  };

  return (
    <Card>
      <CardHeader
        title={t("admin.users.title")}
        description={t("admin.users.help")}
        action={
          <Button onClick={() => setOpen(true)}>
            <UserPlus className="h-4 w-4" /> {t("admin.users.add")}
          </Button>
        }
      />
      <ErrorNote message={error} />
      <div className="divide-y">
        {data?.map((u) => (
          <div key={u.id} className="flex items-center justify-between gap-4 p-5">
            <div>
              <p className="flex items-center gap-2 font-medium">
                {u.username} <Badge title={t(ROLE_HELP[u.role])}>{u.role}</Badge>
              </p>
              <p className="text-sm text-muted-foreground">
                {t("admin.users.last_sign_in", { when: formatDate(u.last_login_at) })}
              </p>
            </div>
            {u.id !== user?.id && u.role !== "owner" && (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => remove(u)}
                aria-label={t("admin.users.delete", { username: u.username })}
              >
                <Trash2 className="h-4 w-4" />
              </Button>
            )}
          </div>
        ))}
      </div>
      <AddUserDialog
        open={open}
        canMakeAdmin={user?.role === "owner"}
        onClose={() => setOpen(false)}
        onAdded={() => {
          setOpen(false);
          reload();
        }}
      />
    </Card>
  );
}

function AddUserDialog(props: { open: boolean; canMakeAdmin: boolean; onClose: () => void; onAdded: () => void }) {
  const { t } = useTranslation();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [role, setRole] = useState<Role>("viewer");
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      await api.post("api/users", { username, password, role });
      setUsername("");
      setPassword("");
      props.onAdded();
    } catch (err) {
      setError((err as Error).message);
    }
  };

  return (
    <Dialog open={props.open} onClose={props.onClose} title={t("admin.users.add")}>
      <form onSubmit={submit} className="space-y-4">
        <Field label={t("auth.username")}>
          <Input value={username} onChange={(e) => setUsername(e.target.value)} required />
        </Field>
        <Field label={t("auth.password")} help={t("admin.users.password_help")}>
          <Input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            minLength={8}
            required
          />
        </Field>
        <Field label={t("admin.users.role")} help={t(ROLE_HELP[role])}>
          <Select value={role} onChange={(e) => setRole(e.target.value as Role)}>
            <option value="viewer">{t("admin.users.option_viewer")}</option>
            <option value="editor">{t("admin.users.option_editor")}</option>
            {props.canMakeAdmin && <option value="admin">{t("admin.users.option_admin")}</option>}
          </Select>
        </Field>
        <ErrorNote message={error} />
        <div className="flex justify-end gap-2">
          <Button type="button" variant="ghost" onClick={props.onClose}>
            {t("common.cancel")}
          </Button>
          <Button type="submit">{t("admin.users.add")}</Button>
        </div>
      </form>
    </Dialog>
  );
}
