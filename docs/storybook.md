# Storybook and visual testing (web app)

The web app's components, starting with the video player, have stories in
Storybook. Each story renders a real component with made-up data, so nothing
needs a Kinora server. The same stories are the web app's component tests.

```
cd web
pnpm storybook        # http://localhost:6006
pnpm test             # unit tests + every story as a test (headless Chromium)
pnpm test:ui          # the same tests in Vitest's own dashboard
pnpm test:visual      # screenshots of "visual" stories on 8 devices, compared with saved ones
```

The first time, install the test browser once: `pnpm exec playwright install chromium`.

## What's where

| Path | What it is |
|---|---|
| `web/.storybook/main.ts` | Storybook config: addons, the sample video folder, the MCP manifest |
| `web/.storybook/preview.tsx` | Theme menu, device list, mocked server, a11y set to fail tests |
| `web/.storybook/devices.ts` | **The device list.** Phones, tablets, laptops, desktops and TVs in CSS pixels. Feeds both the toolbar and the visual tests |
| `web/.storybook/vitest.setup.ts` | Takes the screenshots for `pnpm test:visual` |
| `web/.storybook/public/sample.webm` | A 90 minute gradient clip (136 KB) the player stories play |
| `web/src/stories/` | Shared story helpers: mock data (`fixtures.ts`), fake server (`mock-api.ts`), player wrapper (`PlayerHarness.tsx`) |
| `web/src/**/*.stories.tsx` | The stories, next to their components |

## Checking a component on every device

Pick a device from the viewport menu in Storybook's toolbar (the phone/monitor
icon). The list covers small Android and iPhone SE through Pixel 10, short
landscape phones, iPads in both orientations, laptops, 1080p and ultrawide
monitors, and TVs: a 1080p TV browser renders at 960×540 CSS px, a 4K one at
1920×1080, and a few at 3840×2160. The paintbrush menu next to it switches the
theme.

To add a device, add a line to `devices.ts`. Set `visual: true` to include it
in the screenshot run (keep that set small; each device multiplies the run).

## The test dashboard

In Storybook, the **Run tests** button at the bottom of the sidebar runs every
story through Vitest in headless Chromium and marks each story pass or fail.
Click a story to see its steps in the **Interactions** panel and any
accessibility problems in the **Accessibility** panel. Turn on "Watch" to rerun
tests as you edit, and "Coverage" for a coverage report.

A story fails its test when:

- it throws while rendering,
- its `play` function fails (for example the keyboard shortcut checks in
  `KeyboardShortcuts`), or
- axe finds an accessibility violation. A story can opt out with
  `parameters: { a11y: { test: "todo" } }`, which still lists the problems in
  the panel. Use that only for known issues, with a comment saying which.

`pnpm test:ui` shows the same tests (plus the unit tests) in Vitest's browser
dashboard instead.

## Visual tests

Stories tagged `visual` are screenshotted on every device marked `visual` in
`devices.ts` (eight today) and compared with the screenshots saved in
`__screenshots__/` next to the story file.

```
pnpm test:visual            # compare
pnpm test:visual --update   # accept the current look as the new baseline
```

A new story or device saves its first screenshot and fails once so you look at
it. Screenshots depend on the operating system's fonts, so the file names
include the platform (`-darwin`, `-linux`) and baselines from one OS are never
compared with another. Commit the baselines from the machine you run this on.

CI does not compare screenshots. Its `web-screenshots` job takes them on Linux
and uploads them as a build artifact (`web-screenshots-<run>`), so every pull
request has a gallery of how each visual story looks on each device.

### Hosted alternatives (not set up)

| Option | Cost | Why you might want it |
|---|---|---|
| Chromatic (by the Storybook team) | Free up to 5,000 snapshots a month, paid after | Hosted review UI for diffs on each PR, consistent rendering, no baselines in git |
| Argos, Lost Pixel | Free tiers or self-hostable | Similar PR review flow on top of the screenshots we already take |

Today's setup is free and local. A hosted service is worth it once more than
one person reviews UI changes.

## Writing a story

Copy the closest existing story. The rules that keep tests stable:

- Mock the server with `parameters.api`, for example
  `{ "api/player/style": { style: "pro" } }`. Defaults are in
  `web/src/stories/mock-api.ts`. Never call a real server.
- Put new made-up data in `web/src/stories/fixtures.ts`.
- Anything that changes over time must be fixed: the Pro player's "ends at"
  clock freezes `Date.now` in its story's `beforeEach`. Spinners and playing
  video never settle, so don't tag those stories `visual`.
- Query by role and accessible name (`getByRole("button", { name: "Subtitles" })`),
  the way a screen reader or voice control user finds things.

## AI agents (Storybook MCP)

`@storybook/addon-mcp` runs an MCP server inside the Storybook dev server at
`http://localhost:6006/mcp`. The repo's `.mcp.json` registers it for Claude
Code as `kinora-storybook`, so with `pnpm storybook` running an agent can list
the components and their props, get story-writing instructions, preview
stories, and run the story tests. Storybook doesn't publish separate agent
skills; `.claude/skills/kinora-storybook/SKILL.md` is this project's own short
guide for writing Kinora stories.
