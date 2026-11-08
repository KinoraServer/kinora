---
name: kinora-design
description: Kinora's design language and code style — how the web UI (web/) should look and how its code should be written. Use whenever adding or changing UI in web/: components, pages, styling, theming, tokens, player UI. Also use when the user says "make it match", "make it look right", complains something looks off, or when a design question is ambiguous — the decision log here settles it.
---

# Kinora design and code style

Kinora is a self-hosted media server aimed at TVs and living rooms: a calm, dark,
artwork-first UI that must work as well with a TV remote as with a mouse. This
skill exists so new UI is indistinguishable from existing UI, and so design
choices stay explicit and revisable.

## Before designing or writing UI

Read `references/design-decisions.md` first — it is the log of every deliberate
design choice. If a question isn't covered there, follow the closest precedent
in the code or ask the user, then record the decision (see below). For code
conventions, read `references/code-style.md`.

## Non-negotiables

- **Tokens, not colors.** Every color and corner radius comes from the
  `--kn-*` tokens in `web/src/index.css`, exposed to Tailwind through
  `@theme inline` (`bg-background`, `text-primary`, `rounded-lg`, ...). Never
  write a raw hex or oklch into a component: themes swap tokens at runtime
  (`src/lib/theme.tsx`, `docs/themes.md`), so a hardcoded color breaks every
  theme.
- **Reuse `web/src/components/ui/`** (Button, Card, Dialog, Input) before
  writing new markup. A genuinely new primitive goes in there, in the same
  variant/size style.
- **It must survive every theme.** Before calling a visual change done, check
  it in a dark theme, a light theme, and one extreme (e.g. OLED Black):
  contrast, borders and hover states must all hold.
- **It must work with a keyboard.** TV remotes send keys, not mouse events:
  `focus-visible:ring-2 focus-visible:ring-ring` on every interactive element.
- **Verify visually in Storybook.** Every new or changed component gets a
  story next to it; run `pnpm test`, and for layout changes `pnpm test:visual`
  and look at the screenshots on every device before accepting them
  (`docs/storybook.md`, `kinora-storybook` skill).

## Iterating on this skill

This skill is a living document — updating it is part of the job.

- When a design choice is made (yours or a user correction): append a
  one-line entry to `references/design-decisions.md` — the decision, the why,
  and where it lives in code.
- When a code-style choice is made: update `references/code-style.md`.
- When a decision is later reversed, fix the entry in place. The log must
  always read as the current truth, not as a history of debates.
