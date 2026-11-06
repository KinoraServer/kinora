import { Trash2, UserPlus } from "lucide-react";
import { useState, type FormEvent } from "react";
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

const ROLE_HELP: Record<Role, string> = {
  owner: "Set up the server",
  admin: "Manages libraries, users and settings",
  editor: "Can fix metadata",
  viewer: "Can watch",
};

export function UsersPage() {
  const { user } = useSession();
  const { data, reload, error } = useApi<UserRow[]>("api/users");
  const [open, setOpen] = useState(false);

  const remove = async (u: UserRow) => {
    if (!confirm(`Delete ${u.username}? Their watch history is removed too.`)) return;
    await api.del(`api/users/${u.id}`).catch((e: Error) => alert(e.message));
    reload();
  };

  return (
    <Card>
      <CardHeader
        title="Users"
        description="Everyone who can sign in to this server."
        action={
          <Button onClick={() => setOpen(true)}>
            <UserPlus className="h-4 w-4" /> Add user
          </Button>
        }
      />
      <ErrorNote message={error} />
      <div className="divide-y">
        {data?.map((u) => (
          <div key={u.id} className="flex items-center justify-between gap-4 p-5">
            <div>
              <p className="flex items-center gap-2 font-medium">
                {u.username} <Badge title={ROLE_HELP[u.role]}>{u.role}</Badge>
              </p>
              <p className="text-sm text-muted-foreground">Last sign-in: {formatDate(u.last_login_at)}</p>
            </div>
            {u.id !== user?.id && u.role !== "owner" && (
              <Button variant="ghost" size="sm" onClick={() => remove(u)} aria-label={`Delete ${u.username}`}>
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
    <Dialog open={props.open} onClose={props.onClose} title="Add user">
      <form onSubmit={submit} className="space-y-4">
        <Field label="Username">
          <Input value={username} onChange={(e) => setUsername(e.target.value)} required />
        </Field>
        <Field label="Password" help="At least 8 characters. They can change it later.">
          <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} minLength={8} required />
        </Field>
        <Field label="Role" help={ROLE_HELP[role]}>
          <Select value={role} onChange={(e) => setRole(e.target.value as Role)}>
            <option value="viewer">Viewer</option>
            <option value="editor">Editor</option>
            {props.canMakeAdmin && <option value="admin">Admin</option>}
          </Select>
        </Field>
        <ErrorNote message={error} />
        <div className="flex justify-end gap-2">
          <Button type="button" variant="ghost" onClick={props.onClose}>
            Cancel
          </Button>
          <Button type="submit">Add user</Button>
        </div>
      </form>
    </Dialog>
  );
}
