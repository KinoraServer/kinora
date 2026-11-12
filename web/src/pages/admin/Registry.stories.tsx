import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, waitFor, within } from "storybook/test";
import { RegistryCard, type RegistryPackage, type RegistryView } from "./Registry";

const pkg = (p: Partial<RegistryPackage> & Pick<RegistryPackage, "id" | "kind" | "name" | "description">) => ({
  author: "Kinora",
  license: "GPL-2.0-only",
  homepage: null,
  recommended: false,
  provides: [],
  permissions: {},
  version: "1.0.0",
  size: 4096,
  installed_version: null,
  installed_at: null,
  update_available: false,
  ...p,
});

const packages: RegistryPackage[] = [
  pkg({
    id: "tmdb",
    kind: "module",
    name: "TMDB metadata",
    description: "Titles, descriptions, artwork and ratings for movies and TV from The Movie Database.",
    recommended: true,
    provides: ["metadata_provider"],
    permissions: { http: ["api.themoviedb.org"] },
    size: 175_754,
  }),
  pkg({
    id: "auto-tracks",
    kind: "module",
    name: "Auto audio & subtitles",
    description: "Remembers the audio and subtitle tracks each viewer picks for a show.",
    provides: ["track_selection"],
    permissions: { storage: true },
    version: "1.1.0",
    installed_version: "1.0.0",
    installed_at: 1,
    update_available: true,
  }),
  pkg({ id: "ember", kind: "theme", name: "Ember", description: "A warm dark theme with an orange accent." }),
  pkg({
    id: "marquee",
    kind: "app-skin",
    name: "Marquee",
    description: "A big rotating hero, rails of landscape cards and full-bleed detail pages.",
    installed_version: "1.0.0",
    installed_at: 1,
  }),
  pkg({ id: "neon", kind: "player-skin", name: "Neon", description: "A glowing video player skin." }),
];

const view: RegistryView = {
  url: "https://raw.githubusercontent.com/KinoraServer/kinora-registry/published",
  signed: true,
  error: null,
  packages,
};

/** What the mocked server was last asked to do. */
let lastCall = "";
const record = ({ method, path }: { method: string; path: string }) => {
  lastCall = `${method} ${path}`;
  return { id: "x", version: "1.0.0" };
};

/** Admin > Modules & themes: the list of packages to install from the registry. */
const meta = {
  title: "Admin/Registry",
  component: RegistryCard,
  args: { onChanged: fn() },
  decorators: [
    (Story) => (
      <div className="mx-auto max-w-3xl bg-background p-4 text-foreground">
        <Story />
      </div>
    ),
  ],
  parameters: {
    layout: "fullscreen",
    api: {
      "api/registry": view,
      "POST api/registry/tmdb/install": record,
      "POST api/registry/auto-tracks/install": record,
      "DELETE api/registry/auto-tracks": record,
      "DELETE api/registry/marquee": record,
    },
  },
} satisfies Meta<typeof RegistryCard>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Browse: Story = {
  tags: ["visual"],
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(await c.findByRole("button", { name: "Install TMDB metadata" })).toBeVisible();
    await expect(c.getByText("It can: Connect to api.themoviedb.org.")).toBeVisible();
    await expect(c.getByRole("button", { name: "Update to 1.1.0: Auto audio & subtitles" })).toBeVisible();
  },
};

export const InstallAndFilter: Story = {
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    await userEvent.click(await c.findByRole("button", { name: "Install TMDB metadata" }));
    await waitFor(() => expect(lastCall).toBe("POST api/registry/tmdb/install"));
    await waitFor(() => expect(args.onChanged).toHaveBeenCalled());

    await userEvent.click(c.getByRole("button", { name: "Themes" }));
    await expect(c.getByText("Ember")).toBeVisible();
    await expect(c.queryByText("TMDB metadata")).toBeNull();
  },
};

export const UninstallAModule: Story = {
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await userEvent.click(await c.findByRole("button", { name: "Uninstall Auto audio & subtitles" }));
    const dialog = within(await within(document.body).findByRole("dialog"));
    await userEvent.click(dialog.getByRole("checkbox", { name: "Also delete its settings and saved data" }));
    await userEvent.click(dialog.getByRole("button", { name: "Uninstall" }));
    await waitFor(() => expect(lastCall).toBe("DELETE api/registry/auto-tracks"));
  },
};

/** The registry can't be reached: what is installed is still listed and can be removed. */
export const Offline: Story = {
  parameters: {
    api: {
      "api/registry": {
        ...view,
        signed: false,
        error: "could not reach the registry: error sending request",
        packages: [{ ...packages[3], version: null, size: null }],
      },
    },
  },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(await c.findByText(/can't be read right now/)).toBeVisible();
    await expect(c.getByRole("button", { name: "Uninstall Marquee" })).toBeVisible();
  },
};
