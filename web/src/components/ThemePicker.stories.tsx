import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, userEvent, waitFor, within } from "storybook/test";
import { Card } from "@/components/ui/card";
import { ThemePicker } from "./ThemePicker";

/** The theme picker on Admin → Modules & themes. Picking applies instantly. */
const meta = {
  title: "Components/ThemePicker",
  component: ThemePicker,
  decorators: [
    (Story) => (
      <Card className="max-w-3xl p-5">
        <Story />
      </Card>
    ),
  ],
} satisfies Meta<typeof ThemePicker>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  tags: ["visual"],
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(await c.findByRole("radio", { name: /^Kinora/ })).toBeChecked();
    await expect(c.getByRole("radio", { name: /Dark/ })).toBeChecked();
  },
};

export const SwitchModeAndTheme: Story = {
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await userEvent.click(await c.findByRole("radio", { name: /Light/ }));
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe("kinora-light"));

    // Pairs keep the mode; a dark-only theme switches to dark and locks the mode.
    await userEvent.click(c.getByRole("radio", { name: /Midnight/ }));
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe("daylight"));
    await userEvent.click(c.getByRole("radio", { name: /Ember/ }));
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe("ember"));
    await expect(c.getByRole("radio", { name: /Light/ })).toBeDisabled();
    await expect(c.getByText(/Ember always uses dark mode/)).toBeVisible();
  },
};
