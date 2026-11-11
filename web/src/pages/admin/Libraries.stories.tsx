import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, waitFor, within } from "storybook/test";
import { demoLibraries } from "@/stories/fixtures";
import { LibrariesPage, LibraryDialog } from "./Libraries";

/** Admin > Libraries, and the dialog that adds a library or edits one. */
const meta = {
  title: "Admin/Libraries",
  component: LibrariesPage,
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
      "api/fs/folders": { path: "/media", parent: "/", folders: ["anime", "movies", "movies-4k", "tv"] },
    },
  },
} satisfies Meta<typeof LibrariesPage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const List: Story = {
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(await c.findByRole("button", { name: "Edit Movies" })).toBeVisible();
  },
};

/** The last edit the mocked server received. */
let lastEdit: unknown = null;

const movies = { ...demoLibraries[0], paths: ["/media/movies", "/media/movies-4k"] };

export const EditLibrary: StoryObj<typeof LibraryDialog> = {
  render: (args) => <LibraryDialog {...args} />,
  args: { library: movies, onClose: fn(), onSaved: fn() },
  parameters: {
    api: {
      "api/fs/folders": { path: "/media", parent: "/", folders: ["anime", "movies", "movies-4k", "tv"] },
      "PUT api/libraries/lib-movies": ({ body }: { body: unknown }) => {
        lastEdit = body;
        return { ok: true, rescan: true };
      },
    },
  },
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    const name = c.getByLabelText("Name");
    await expect(name).toHaveValue("Movies");
    await expect(c.getByLabelText("Type")).toBeDisabled();
    await expect(c.getByRole("button", { name: "Save" })).toBeVisible();

    await userEvent.clear(name);
    await userEvent.type(name, "Films");
    await userEvent.click(c.getByRole("button", { name: "Remove /media/movies-4k" }));
    await expect(c.getByRole("status")).toHaveTextContent("removed folder");
    await userEvent.click(c.getByRole("button", { name: "Save and scan" }));
    await waitFor(() => expect(args.onSaved).toHaveBeenCalled());
    await expect(lastEdit).toEqual({ name: "Films", paths: ["/media/movies"] });
  },
};

export const AddLibrary: StoryObj<typeof LibraryDialog> = {
  render: (args) => <LibraryDialog {...args} />,
  args: { library: null, onClose: fn(), onSaved: fn() },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(c.getByLabelText("Type")).toBeEnabled();
    await expect(c.getByRole("button", { name: "Add and scan" })).toBeDisabled();
  },
};
