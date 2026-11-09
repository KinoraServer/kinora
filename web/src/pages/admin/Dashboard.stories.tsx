import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, userEvent, within } from "storybook/test";
import { liveStreams, liveStreamsNow } from "@/stories/fixtures";
import { StreamsView } from "./Dashboard";

/**
 * Admin > Dashboard: every live playback as a card. Fixtures have no artwork,
 * so posters show the placeholder.
 */
const meta = {
  title: "Admin/Dashboard",
  component: StreamsView,
  decorators: [
    (Story) => (
      <div className="mx-auto max-w-5xl bg-background p-4 text-foreground">
        <Story />
      </div>
    ),
  ],
  parameters: { layout: "fullscreen" },
  args: { streams: liveStreams, fetchedAt: liveStreamsNow, now: liveStreamsNow },
} satisfies Meta<typeof StreamsView>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Collapsed: Story = {
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(c.getByText("45:07 of 58:21")).toBeVisible();
    await expect(c.getByText("1hr 39min of 2hr 46min")).toBeVisible();
    await expect(c.getByText("2hr 56min of 3hr 10min")).toBeVisible();
    await expect(c.getAllByText("Direct Play").length).toBeGreaterThan(1);
  },
};

export const InfoAndTraceOpen: Story = {
  args: {
    defaultOpen: {
      [liveStreams[0].id]: { info: true, trace: true },
      [liveStreams[2].id]: { info: true },
    },
  },
};

export const ToggleSections: Story = {
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    const [info] = c.getAllByRole("button", { name: /Info/ });
    await userEvent.click(info);
    await expect(info).toHaveAttribute("aria-expanded", "true");
    await expect(c.getByText("Dolby Vision 8.1 → SDR")).toBeVisible();
    const [trace] = c.getAllByRole("button", { name: /Trace/ });
    await userEvent.click(trace);
    await expect(c.getByText("2.4× · 58 fps")).toBeVisible();
  },
};

export const Empty: Story = { args: { streams: [] } };
