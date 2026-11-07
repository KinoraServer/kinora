import { useState } from "react";
import { avatarUrl } from "@/lib/api";
import type { User } from "@/lib/types";
import { cn } from "@/lib/utils";

/** A user's profile picture, or their first letter when they haven't set one. */
export function Avatar({
  user,
  className,
}: {
  user: Pick<User, "id" | "username" | "avatar_version">;
  className?: string;
}) {
  const src = avatarUrl(user);
  // Remember which picture failed so a new one is tried again.
  const [failed, setFailed] = useState<string | null>(null);
  return (
    <span
      className={cn(
        "flex h-9 w-9 shrink-0 items-center justify-center overflow-hidden rounded-full bg-accent text-sm font-semibold text-accent-foreground uppercase",
        className,
      )}
    >
      {src && failed !== src ? (
        <img src={src} alt="" className="h-full w-full object-cover" onError={() => setFailed(src)} />
      ) : (
        user.username.slice(0, 1)
      )}
    </span>
  );
}
