// The line between a skin and Kinora's own UI. Kept tiny so the main bundle
// can use it without pulling in the skin engine.

import { createContext, useContext, type ReactNode } from "react";

/** True inside a skinned shell: Kinora's own pages then get a boundary the skin's CSS can't cross. */
export const InSkinShell = createContext(false);

/** Kinora's own page, fenced off from the skin's CSS when it shows inside a skin's shell. */
export function DefaultPage({ children }: { children: ReactNode }) {
  const inShell = useContext(InSkinShell);
  if (!inShell) return <>{children}</>;
  return (
    <div
      data-kn-standard
      className="mx-auto min-h-[60vh] max-w-[1800px] bg-background px-4 py-6 text-foreground sm:px-8"
    >
      {children}
    </div>
  );
}
