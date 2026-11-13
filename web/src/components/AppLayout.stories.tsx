import type { Meta, StoryObj } from "@storybook/react-vite";
import { createMemoryRouter, RouterProvider } from "react-router";
import { expect, userEvent, within } from "storybook/test";
import { SessionProvider } from "@/lib/session";
import { demoEpisodes, demoMovies, demoUser } from "@/stories/fixtures";
import { AppLayout } from "./AppLayout";
import { MediaRow } from "./ItemTile";

function Page() {
  return (
    <div className="space-y-8">
      <MediaRow title="Continue watching" items={demoEpisodes} wide />
      <MediaRow title="Recently added movies" items={demoMovies} />
    </div>
  );
}

function App() {
  const router = createMemoryRouter([{ element: <AppLayout />, children: [{ index: true, element: <Page /> }] }]);
  return (
    <SessionProvider>
      <RouterProvider router={router} />
    </SessionProvider>
  );
}

/** The header and page frame, signed in as an owner, with three mocked libraries. */
const meta = {
  title: "App/AppLayout",
  component: App,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof App>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Home: Story = {
  tags: ["visual"],
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(await c.findByRole("link", { name: /TV Shows/ })).toBeVisible();
    await userEvent.click(await c.findByRole("button", { name: "Account menu" }));
    await expect(c.getByRole("link", { name: /Admin Settings/ })).toBeVisible();
  },
};

export const AccountMenu: Story = {
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await userEvent.click(await c.findByRole("button", { name: "Account menu" }));
    await expect(c.getByRole("link", { name: /User Settings/ })).toBeVisible();
    await expect(c.getByRole("link", { name: /Admin Settings/ })).toBeVisible();
    await expect(c.getByRole("link", { name: /Report an issue/ })).toBeVisible();
    await expect(c.getByRole("button", { name: /Sign out/ })).toBeVisible();
    await expect(c.queryByText("Theme")).not.toBeInTheDocument();
  },
};

/** Viewers don't get the Admin Settings link. */
export const AccountMenuViewer: Story = {
  parameters: { api: { "api/auth/me": { ...demoUser, role: "viewer" } } },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await userEvent.click(await c.findByRole("button", { name: "Account menu" }));
    await expect(c.getByRole("link", { name: /User Settings/ })).toBeVisible();
    await expect(c.queryByRole("link", { name: /Admin Settings/ })).not.toBeInTheDocument();
  },
};
