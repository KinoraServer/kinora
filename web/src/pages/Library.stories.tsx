import type { Meta, StoryObj } from "@storybook/react-vite";
import { createMemoryRouter, RouterProvider } from "react-router";
import { expect, userEvent, waitFor, within } from "storybook/test";
import { AppLayout } from "@/components/AppLayout";
import { SessionProvider } from "@/lib/session";
import type { ItemCard } from "@/lib/types";
import { LibraryPage } from "./Library";

// A big movie library, sorted like the server sorts it (ignoring "The", "A", "An").
// No titles start with Q, X or Z, so those letters show dimmed on the rail.
const WORDS = [
  "2001 Nights",
  "9 Lives",
  "Arrival Gate",
  "The Amber Coast",
  "Atlas",
  "Blue Hour",
  "Brightwater",
  "Black Orchard",
  "Cinder",
  "Coldharbour",
  "A Crowded Room",
  "Delta Run",
  "Dust Bowl",
  "Driftwood",
  "Echoes",
  "Ember",
  "Eastbound",
  "Fable",
  "Fallow Year",
  "Glasshouse",
  "The Grey Line",
  "Granite",
  "Harbour Lights",
  "Hollow Pines",
  "Iron Bay",
  "Island Fever",
  "Jade",
  "Juniper",
  "Kestrel",
  "Kingfisher",
  "The Long Drive Home",
  "Lowlands",
  "Midnight Ferry",
  "Marigold",
  "Meridian",
  "Northern Lights",
  "Nightjar",
  "Orbit",
  "Open Water",
  "Paper Moon",
  "Pale Fire",
  "Redwood",
  "Riverside",
  "Static",
  "Saltmarsh",
  "Silver Screen",
  "Tidewater",
  "Thornfield",
  "Undertow",
  "Upper Room",
  "Velvet",
  "Vanishing Point",
  "Wildflower",
  "Winterlight",
  "Yellowstone Road",
  "Young Hearts",
];
const sortKey = (t: string) => t.toLowerCase().replace(/^(the|a|an) /, "");
const movies: ItemCard[] = WORDS.flatMap((w, i) =>
  [1, 2, 3].map((n) => ({
    id: `m${i}-${n}`,
    library_id: "lib-movies",
    kind: "movie" as const,
    parent_id: null,
    series_id: null,
    title: n === 1 ? w : `${w} ${n === 2 ? "II" : "III"}`,
    year: 1990 + ((i * 7 + n) % 35),
    season_number: null,
    episode_number: null,
    series_title: null,
    has_poster: false,
    has_backdrop: false,
    has_thumb: false,
    duration_ms: null,
    position_ms: null,
    played: null,
    rating: null,
  })),
).sort((a, b) => sortKey(a.title).localeCompare(sortKey(b.title)));

function App() {
  const router = createMemoryRouter(
    [{ element: <AppLayout />, children: [{ path: "library/:id", element: <LibraryPage /> }] }],
    { initialEntries: ["/library/lib-movies"] },
  );
  return (
    <SessionProvider>
      <RouterProvider router={router} />
    </SessionProvider>
  );
}

/** A long library sorted by title, with the A–Z rail on the right. */
const meta = {
  title: "Library/LibraryPage",
  component: App,
  parameters: { layout: "fullscreen", api: { "api/items": movies } },
} satisfies Meta<typeof App>;

export default meta;
type Story = StoryObj<typeof meta>;

export const LetterRail: Story = {
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    const rail = await c.findByRole("navigation", { name: "Jump to letter" });
    const r = within(rail);
    await expect(r.getByRole("button", { name: "Jump to Q" })).toHaveAttribute("aria-disabled", "true");
    r.getByRole("button", { name: "Jump to M" }).focus();
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(r.getByRole("button", { name: "Jump to M" })).toHaveAttribute("aria-current", "true"));
    await waitFor(() => expect(window.scrollY).toBeGreaterThan(0));
  },
};
