import type { Meta, StoryObj } from "@storybook/react-vite";
import { useEffect, useState } from "react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { expect, within } from "storybook/test";
import { SessionProvider } from "@/lib/session";
import { HomePage } from "@/pages/Home";
import { ItemPage } from "@/pages/Item";
import { LibraryPage, SearchPage } from "@/pages/Library";
import { UserSettingsPage } from "@/pages/settings/UserSettings";
import { skinApi, starterBundle, type Starter } from "@/stories/app-skins";
import { mockError } from "@/stories/mock-api";
import { DefaultPage, SkinLayout, SkinPage } from "./routes";
import { SkinProvider } from "./SkinProvider";
import type { Device } from "./types";

/** The whole app with a starter skin, on one route. */
function SkinnedApp({ path, device }: { skin: Starter; path: string; device?: Device }) {
  // The device has to be forced before the provider first reads it.
  const [ready, setReady] = useState(false);
  useEffect(() => {
    if (device) sessionStorage.setItem("kinora.skin-device", device);
    else sessionStorage.removeItem("kinora.skin-device");
    setReady(true);
    return () => sessionStorage.removeItem("kinora.skin-device");
  }, [device]);
  if (!ready) return null;
  const router = createMemoryRouter(
    [
      {
        element: <SkinLayout />,
        children: [
          {
            index: true,
            element: (
              <SkinPage page="home">
                <HomePage />
              </SkinPage>
            ),
          },
          {
            path: "library/:id",
            element: (
              <SkinPage page="library">
                <LibraryPage />
              </SkinPage>
            ),
          },
          {
            path: "item/:id",
            element: (
              <SkinPage page="item">
                <ItemPage />
              </SkinPage>
            ),
          },
          {
            path: "search",
            element: (
              <SkinPage page="search">
                <SearchPage />
              </SkinPage>
            ),
          },
          {
            path: "settings",
            element: (
              <DefaultPage>
                <UserSettingsPage />
              </DefaultPage>
            ),
          },
        ],
      },
    ],
    { initialEntries: [path] },
  );
  return (
    <SessionProvider>
      <SkinProvider>
        <RouterProvider router={router} />
      </SkinProvider>
    </SessionProvider>
  );
}

/**
 * The app skin playground: the starter skins (from
 * crates/server/src/modules/app_skin_samples) rendering the real app with
 * mocked data. Edit a starter's files and the story updates. Pick a screen
 * size from the toolbar; the TV stories also force TV mode.
 */
const meta = {
  title: "App skins/Playground",
  component: SkinnedApp,
  parameters: { layout: "fullscreen" },
  args: { skin: "marquee", path: "/" },
  render: (args) => <SkinnedApp key={`${args.skin}${args.path}${args.device}`} {...args} />,
} satisfies Meta<typeof SkinnedApp>;

export default meta;
type Story = StoryObj<typeof meta>;

const skinRoot = (el: HTMLElement) => el.ownerDocument.querySelector("[data-app-skin]");

/** Every card in the "Continue watching" row has the same width at rest, long titles or not. */
async function expectEvenContinueRow(el: HTMLElement) {
  const c = within(el);
  const heading = await c.findByRole("heading", { name: "Continue watching" });
  const row = heading.closest("section");
  const cards = [...(row?.querySelectorAll("a") ?? [])];
  await expect(cards.length).toBeGreaterThan(2);
  const widths = new Set(cards.map((a) => Math.round(a.getBoundingClientRect().width)));
  await expect([...widths]).toHaveLength(1);
}

export const MarqueeHome: Story = {
  parameters: { api: skinApi(starterBundle("marquee")) },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(await c.findByText("Recently added in Movies")).toBeVisible();
    await expect(skinRoot(canvasElement)?.getAttribute("data-app-skin")).toBe("marquee");
    await expectEvenContinueRow(canvasElement);
  },
};

/** Kinora's own home page with the same data, no skin active. */
export const DefaultHome: Story = {
  parameters: { api: { ...skinApi(starterBundle("marquee")), "api/app-skin/active": mockError(404) } },
  play: async ({ canvasElement }) => {
    await expect(skinRoot(canvasElement)).toBeNull();
    await expectEvenContinueRow(canvasElement);
  },
};

export const MarqueeSeries: Story = {
  args: { path: "/item/s1" },
  parameters: { api: skinApi(starterBundle("marquee")) },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(await c.findByRole("heading", { name: "Signal" })).toBeVisible();
  },
};

export const MarqueeLibrary: Story = {
  args: { path: "/library/lib-movies" },
  parameters: { api: skinApi(starterBundle("marquee", { "card-size": "l" })) },
};

export const MarqueeTv: Story = {
  args: { device: "tv" },
  parameters: { api: skinApi(starterBundle("marquee")) },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await c.findByText("Recently added in Movies");
    await expect(skinRoot(canvasElement)?.getAttribute("data-device")).toBe("tv");
  },
};

/** Kinora's own page (settings) inside the skin's shell, fenced off from the skin's CSS. */
export const MarqueeStandardPage: Story = {
  args: { path: "/settings" },
  parameters: { api: skinApi(starterBundle("marquee")) },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(await c.findByRole("link", { name: /Profile/ })).toBeVisible();
    await expect(canvasElement.ownerDocument.querySelector("[data-kn-standard]")).not.toBeNull();
  },
};

export const BlankHome: Story = {
  args: { skin: "blank" },
  parameters: { api: skinApi(starterBundle("blank")) },
  play: async ({ canvasElement }) => {
    await expect(await within(canvasElement).findByText("Recently added in Movies")).toBeVisible();
  },
};
