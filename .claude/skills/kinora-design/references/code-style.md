# Code style (web/)

Conventions beyond what tools already enforce. Enforced parts: Prettier (git
hook, `pnpm format`), `tsc --noEmit` (`pnpm typecheck`), `pnpm test` (Vitest
unit tests + every Storybook story, including accessibility), Conventional
Commits (commit-msg hook). Match those first, then this file.

## Components

- File layout: `ComponentName.tsx` next to `ComponentName.stories.tsx`;
  shared components in `web/src/components/`, routes in `web/src/pages/`.
- Reusable primitives live in `web/src/components/ui/` and follow the Button
  pattern (`web/src/components/ui/button.tsx`): `forwardRef`, `variant` /
  `size` union types, a class-string `Record` per variant, class merging via
  `cn()` from `@/lib/utils`, and `displayName` set.
- Class lists: base classes, focus/disabled states, variant, size, then the
  caller's `className` — each on its own line inside `cn(...)`.
- Icons: `lucide-react` is the project's icon set. Don't inline new SVGs
  unless the set genuinely can't cover it.
- Keyboard shortcuts go through `@tanstack/react-hotkeys`.

## Tests and stories

- Pure logic in `web/src/lib/` gets a colocated `*.test.ts`
  (see `caps.test.ts`, `format.test.ts`).
- Stories mock the API with `parameters.api` (`web/src/stories/mock-api.ts`)
  and use made-up data from `web/src/stories/fixtures.ts` — never a real
  server.
- Tag a story `visual` only when it renders identically every time.
- Devices live in `web/.storybook/devices.ts`; never hardcode a device or
  breakpoint inside a story.

## Naming and structure

- (record decisions here as they're made)
