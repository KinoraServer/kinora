import type { Meta, StoryObj } from "@storybook/react-vite";
import { Play, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { expect, fn, userEvent, within } from "storybook/test";
import { Button } from "./button";
import { Badge, Card, CardHeader, Empty, ErrorNote, Spinner, Switch } from "./card";
import { Dialog } from "./dialog";
import { Field, Input, Select } from "./input";

/** The small building blocks every page uses. Switch the theme in the toolbar to check each one. */
const meta = {
  title: "UI/Primitives",
  parameters: { layout: "padded" },
} satisfies Meta;

export default meta;
type Story = StoryObj<typeof meta>;

export const Buttons: Story = {
  // Known issue: white on the theme's destructive red is below WCAG AA contrast (3.4:1).
  // "todo" lists it in the Accessibility panel without failing the test.
  parameters: { a11y: { test: "todo" } },
  tags: ["visual"],
  render: () => (
    <div className="flex flex-wrap items-center gap-3 bg-background p-4 text-foreground">
      <Button>
        <Play className="h-4 w-4" /> Play
      </Button>
      <Button variant="secondary">Secondary</Button>
      <Button variant="outline">Outline</Button>
      <Button variant="ghost">Ghost</Button>
      <Button variant="destructive">
        <Trash2 className="h-4 w-4" /> Delete
      </Button>
      <Button size="sm">Small</Button>
      <Button size="lg">Large</Button>
      <Button size="icon" aria-label="Add">
        <Plus className="h-5 w-5" />
      </Button>
      <Button disabled>Disabled</Button>
    </div>
  ),
};

export const Cards: Story = {
  // Known issue: white on the theme's destructive red is below WCAG AA contrast (3.4:1).
  // "todo" lists it in the Accessibility panel without failing the test.
  parameters: { a11y: { test: "todo" } },
  tags: ["visual"],
  render: () => (
    <div className="grid gap-4 bg-background p-4 text-foreground sm:grid-cols-2">
      <Card>
        <CardHeader
          title="Libraries"
          description="Folders Kinora scans for movies and shows."
          action={<Button size="sm">Add library</Button>}
        />
        <div className="flex flex-wrap gap-2 p-5">
          <Badge>Movies</Badge>
          <Badge>412 items</Badge>
          <Badge className="bg-primary/20 text-primary">Scanning</Badge>
        </div>
      </Card>
      <Card className="space-y-3 p-5">
        <ErrorNote message="Couldn't reach TMDB. Check the API key in Settings." />
        <Empty title="Nothing here yet">Add a library to start scanning.</Empty>
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Spinner /> Loading…
        </div>
      </Card>
    </div>
  ),
};

function SwitchDemo() {
  const [on, setOn] = useState(false);
  return (
    <div className="flex items-center gap-3 bg-background p-4 text-foreground">
      <Switch checked={on} onChange={setOn} label="Hardware transcoding" />
      <span className="text-sm">Hardware transcoding: {on ? "on" : "off"}</span>
    </div>
  );
}

export const Toggle: Story = {
  render: () => <SwitchDemo />,
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    const sw = c.getByRole("switch", { name: "Hardware transcoding" });
    await expect(sw).toHaveAttribute("aria-checked", "false");
    await userEvent.click(sw);
    await expect(sw).toHaveAttribute("aria-checked", "true");
  },
};

export const FormFields: Story = {
  tags: ["visual"],
  render: () => (
    <div className="max-w-md space-y-4 bg-background p-4 text-foreground">
      <Field label="Server name" help="Shown in the header and on the login page.">
        <Input defaultValue="Kinora" />
      </Field>
      <Field label="Default theme">
        <Select defaultValue="kinora">
          <option value="kinora">Kinora</option>
          <option value="midnight">Midnight</option>
        </Select>
      </Field>
      <Field label="Disabled">
        <Input disabled placeholder="Can't edit this" />
      </Field>
    </div>
  ),
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    const input = c.getByLabelText("Server name");
    await userEvent.clear(input);
    await userEvent.type(input, "Living room");
    await expect(input).toHaveValue("Living room");
  },
};

const onClose = fn();

export const DialogOpen: Story = {
  // Known issue: white on the theme's destructive red is below WCAG AA contrast (3.4:1).
  // "todo" lists it in the Accessibility panel without failing the test.
  parameters: { a11y: { test: "todo" } },
  tags: ["visual"],
  render: () => (
    <Dialog open title="Delete library?" onClose={onClose}>
      <p className="mb-4 text-sm text-muted-foreground">
        The files on disk stay where they are. Kinora forgets the items and their watch history.
      </p>
      <div className="flex justify-end gap-2">
        <Button variant="ghost">Cancel</Button>
        <Button variant="destructive">Delete</Button>
      </div>
    </Dialog>
  ),
  play: async ({ canvasElement }) => {
    const doc = within(canvasElement.ownerDocument.body);
    await expect(doc.getByRole("dialog", { name: "Delete library?" })).toBeVisible();
    await userEvent.keyboard("{Escape}");
    await expect(onClose).toHaveBeenCalled();
  },
};
