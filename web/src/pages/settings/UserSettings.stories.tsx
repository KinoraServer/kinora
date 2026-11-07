import type { Meta, StoryObj } from "@storybook/react-vite";
import { createMemoryRouter, RouterProvider } from "react-router";
import { expect, userEvent, within } from "storybook/test";
import { SessionProvider } from "@/lib/session";
import { demoUser } from "@/stories/fixtures";
import { UserSettingsPage, UserSettingsSectionPage } from "./UserSettings";

function App({ path }: { path: string }) {
  const router = createMemoryRouter(
    [
      { path: "/settings", element: <UserSettingsPage /> },
      { path: "/settings/:section", element: <UserSettingsSectionPage /> },
    ],
    { initialEntries: [path] },
  );
  return (
    <SessionProvider>
      <div className="p-6">
        <RouterProvider router={router} />
      </div>
    </SessionProvider>
  );
}

/** User Settings: the section list and the Profile page. */
const meta = {
  title: "Pages/UserSettings",
  component: App,
  parameters: { layout: "fullscreen" },
  args: { path: "/settings" },
} satisfies Meta<typeof App>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Sections: Story = {
  tags: ["visual"],
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    for (const name of ["Profile", "Display", "Audio", "Subtitles"]) {
      await expect(await c.findByRole("link", { name: new RegExp(name) })).toBeVisible();
    }
  },
};

export const Profile: Story = {
  tags: ["visual"],
  args: { path: "/settings/profile" },
  parameters: {
    api: {
      "PATCH api/auth/profile": ({ body }: { body: { username: string } }) => ({
        ...demoUser,
        username: body.username,
      }),
      "POST api/auth/password": { ok: true },
    },
  },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    const name = await c.findByLabelText("Username");
    await userEvent.clear(name);
    await userEvent.type(name, "sam");
    await userEvent.click(c.getByRole("button", { name: "Save" }));
    await expect(await c.findByText("Saved.")).toBeVisible();

    await userEvent.type(c.getByLabelText("Current password"), "password123");
    await userEvent.type(c.getByLabelText("New password"), "newpass123");
    await userEvent.type(c.getByLabelText("Confirm new password"), "different1");
    await userEvent.click(c.getByRole("button", { name: "Change password" }));
    await expect(await c.findByText("The new passwords don't match.")).toBeVisible();
    await userEvent.clear(c.getByLabelText("Confirm new password"));
    await userEvent.type(c.getByLabelText("Confirm new password"), "newpass123");
    await userEvent.click(c.getByRole("button", { name: "Change password" }));
    await expect(await c.findByText("Password changed.")).toBeVisible();
    await expect(c.getByLabelText("Current password")).toHaveValue("");
  },
};

export const ComingSoon: Story = {
  args: { path: "/settings/audio" },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(await c.findByText("Audio settings are coming")).toBeVisible();
  },
};
