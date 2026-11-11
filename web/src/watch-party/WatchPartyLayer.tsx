import { WatchPartyPanel, WatchPartyToast } from "./WatchPartyPanel";
import { useWatchParty } from "./WatchPartyProvider";

/**
 * The panel and notes on every page but the player, which shows its own inside its (possibly fullscreen) frame.
 */
export function WatchPartyLayer() {
  const c = useWatchParty();
  if (!c || c.playerPages > 0) return null;
  return (
    <>
      <WatchPartyPanel />
      <WatchPartyToast />
    </>
  );
}
