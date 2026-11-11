import type { Meta, StoryObj } from "@storybook/react-vite";
import { useEffect } from "react";
import { expect, within } from "storybook/test";
import { SessionProvider } from "@/lib/session";
import { WatchPartyButton } from "./WatchPartyPanel";
import { WatchPartyLayer } from "./WatchPartyLayer";
import { useWatchParty, WatchPartyProvider } from "./WatchPartyProvider";

function OpenPanel() {
  const c = useWatchParty();
  useEffect(() => c?.setPanel(true), [c]);
  return null;
}

/** The watch party panel before joining: start a party or join one with a code. Never connects to a relay. */
function App() {
  return (
    <SessionProvider>
      <WatchPartyProvider navigate={() => {}}>
        <div className="p-6">
          <WatchPartyButton />
        </div>
        <OpenPanel />
        <WatchPartyLayer />
      </WatchPartyProvider>
    </SessionProvider>
  );
}

const meta = {
  title: "Watch party/Panel",
  component: App,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof App>;

export default meta;
type Story = StoryObj<typeof meta>;

/** With the public relay: the panel says so and links the relay's privacy notice. */
export const PublicRelay: Story = {
  parameters: { api: { "api/watch-party": { enabled: true, relay_url: null, client: "Kinora/0.1.0" } } },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(await c.findByRole("button", { name: "Start a party" })).toBeVisible();
    await expect(c.getByLabelText("Join with a code")).toBeVisible();
    await expect(c.getByRole("link", { name: "privacy notice" })).toHaveAttribute(
      "href",
      "https://openwatch.party/privacy",
    );
  },
};

/** With the server's own relay. */
export const OwnRelay: Story = {
  parameters: {
    api: { "api/watch-party": { enabled: true, relay_url: "http://127.0.0.1:9", client: "Kinora/0.1.0" } },
  },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(await c.findByText("Watch parties use this server's own relay.")).toBeVisible();
  },
};

/** Module off: no button, no panel. */
export const ModuleOff: Story = {
  parameters: { api: { "api/watch-party": { enabled: false, relay_url: null, client: "Kinora/0.1.0" } } },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await new Promise((r) => setTimeout(r, 200));
    await expect(c.queryByRole("button", { name: "Watch party" })).toBeNull();
  },
};
