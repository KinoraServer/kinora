// The list of built-in player styles and how the server-wide choice is
// resolved. Kept apart from MediaPlayer.tsx so the admin page can show the
// options without pulling the whole player into its bundle.

export interface PlayerStyleInfo {
  id: string;
  name: string;
  description: string;
}

export const BUILTIN_STYLES: PlayerStyleInfo[] = [
  { id: "minimal", name: "Minimal", description: "A slim bar along the bottom." },
  { id: "cinematic", name: "Cinematic", description: "Big centred controls and large type." },
  { id: "floating", name: "Floating", description: "A rounded glass bar floating over the video." },
  { id: "pro", name: "Pro", description: "Dense controls with labels and extra readouts." },
];

export const DEFAULT_STYLE = "minimal";
/** The server setting that means "use whatever the theme says". */
export const THEME_STYLE = "theme";

/** GET/PUT body of `api/player/style`. */
export interface PlayerStyleSetting {
  style: string;
}

/**
 * Which layout to show. The admin's pick (stored on the server) wins; "theme"
 * (the default) means the app skin's player when one is active, else the
 * theme's `player.layout`; anything unknown (a skin that was removed) falls
 * back to the default layout.
 */
export function resolvePlayerStyle(
  choice: string | null | undefined,
  themeLayout: string | null | undefined,
  skinIds: string[],
  appSkinPlayer?: string | null,
): string {
  const wanted = choice && choice !== THEME_STYLE ? choice : (appSkinPlayer ?? themeLayout ?? DEFAULT_STYLE);
  const known = BUILTIN_STYLES.some((s) => s.id === wanted) || skinIds.includes(wanted);
  return known ? wanted : DEFAULT_STYLE;
}
