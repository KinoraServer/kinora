import type { Meta, StoryObj } from "@storybook/react-vite";
import { MemoryRouter } from "react-router";
import { expect, within } from "storybook/test";
import { demoEpisodes, demoMovies } from "@/stories/fixtures";
import { ItemGrid, ItemTile, MediaRow } from "./ItemTile";

/**
 * Posters and episode thumbnails. The fixtures have no artwork, so every tile
 * shows the placeholder; that's also what a fresh library looks like before
 * TMDB fills it in.
 */
const meta = {
  title: "Library/ItemTile",
  component: ItemTile,
  decorators: [
    (Story) => (
      <MemoryRouter>
        <div className="bg-background p-4 text-foreground">
          <Story />
        </div>
      </MemoryRouter>
    ),
  ],
  parameters: { layout: "fullscreen" },
  args: { item: demoMovies[0] },
} satisfies Meta<typeof ItemTile>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Poster: Story = { args: { className: "w-40" } };

export const InProgress: Story = { args: { item: demoMovies[3], className: "w-40" } };

export const Episode: Story = {
  args: { item: demoEpisodes[0], className: "w-64" },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(c.getByText("Signal")).toBeVisible();
    await expect(c.getByText(/S01E01/)).toBeVisible();
  },
};

/** A home page row: scrolls sideways on small screens. */
export const Rows: Story = {
  tags: ["visual"],
  render: () => (
    <div className="space-y-8">
      <MediaRow title="Continue watching" items={demoEpisodes} wide />
      <MediaRow title="Recently added movies" items={demoMovies} />
    </div>
  ),
};

/** A library page: as many columns as fit. */
export const Grid: Story = {
  tags: ["visual"],
  render: () => <ItemGrid items={[...demoMovies, ...demoMovies.map((m) => ({ ...m, id: `${m.id}-b` }))]} />,
};
