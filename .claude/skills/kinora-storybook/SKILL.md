---
name: kinora-storybook
description: Write or update Storybook stories and visual tests for Kinora's web app (web/), especially the video player. Use when adding a component, changing how one looks, or checking layouts on phones, tablets and TVs.
---

# Kinora Storybook stories

Full guide: `docs/storybook.md`. The short version:

1. If the `kinora-storybook` MCP server is available (it needs `cd web && pnpm storybook` running), call its
   `get-storybook-story-instructions` tool first, and use `docs-list` / `docs-show` instead of guessing a
   component's props.
2. Put the story next to the component as `Name.stories.tsx`. Copy the closest existing one
   (`web/src/components/MediaPlayer.stories.tsx` for anything in the player).
3. No real server. Mock responses with `parameters.api` (`web/src/stories/mock-api.ts`), and put made-up data
   in `web/src/stories/fixtures.ts`. Player stories render `PlayerHarness`, which plays the local sample video.
4. Give each behaviour a `play` function that checks it, querying by role and accessible name.
5. Tag a story `visual` only when it renders the same every time: no spinner, no playing video, no current
   time (freeze `Date.now` in the story's `beforeEach`, as the `Pro` story does).
6. Run `cd web && pnpm test` (all stories must pass, including accessibility) and, for layout changes,
   `pnpm test:visual`. Look at the new or changed screenshots in `__screenshots__/` on every device before
   accepting them with `pnpm test:visual --update`.
7. Devices live in `web/.storybook/devices.ts`. Add new ones there, not in a story.
