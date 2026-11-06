import { Film } from "lucide-react";
import { useState } from "react";
import { imageUrl } from "@/lib/api";
import { cn } from "@/lib/utils";

/** An item image with a tasteful placeholder when there's no artwork. */
export function Artwork({
  id,
  kind,
  available,
  title,
  className,
}: {
  id: string;
  kind: "poster" | "backdrop" | "thumb";
  available: boolean;
  title: string;
  className?: string;
}) {
  const [failed, setFailed] = useState(false);
  const show = available && !failed;
  return (
    <div className={cn("relative overflow-hidden bg-muted", className)}>
      {show ? (
        <img
          src={imageUrl(id, kind)}
          alt=""
          loading="lazy"
          onError={() => setFailed(true)}
          className="absolute inset-0 h-full w-full object-cover"
        />
      ) : (
        <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 bg-gradient-to-br from-accent to-muted p-3 text-center">
          <Film className="h-6 w-6 text-muted-foreground" />
          <span className="line-clamp-3 text-xs font-medium text-muted-foreground">{title}</span>
        </div>
      )}
    </div>
  );
}
