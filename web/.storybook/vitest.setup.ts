// Extra setup for the Storybook test projects in vite.config.ts.
//
// The visual projects (`pnpm test:visual`) run each story tagged "visual" at
// one device size, then compare a screenshot of the page with the saved one in
// __screenshots__/ next to the story file. A new story or device saves its
// first screenshot and fails once so you look at it; `pnpm test:visual -u`
// accepts changes on purpose.

import { afterEach, expect, inject } from "vitest";
import { page } from "vitest/browser";

declare module "vitest" {
  export interface ProvidedContext {
    visualDevice?: string;
  }
}

/** Wait until every <video> on the page shows a frame, so screenshots aren't blank. */
async function videosReady(timeoutMs = 5000) {
  const videos = Array.from(document.querySelectorAll("video")).filter((v) => v.currentSrc);
  await Promise.all(
    videos.map(
      (v) =>
        new Promise<void>((resolve) => {
          if (v.readyState >= 2) return resolve();
          const done = () => resolve();
          v.addEventListener("loadeddata", done, { once: true });
          v.addEventListener("error", done, { once: true });
          setTimeout(done, timeoutMs);
        }),
    ),
  );
}

afterEach(async (ctx) => {
  const device = inject("visualDevice");
  const story = (ctx as unknown as { story?: { id: string; tags: string[] } }).story;
  if (!device || !story?.tags.includes("visual") || ctx.task.result?.state === "fail") return;

  await videosReady();
  await document.fonts.ready;
  // Screenshot what's on screen (the first view of the page), not the whole scroll height.
  const root = document.documentElement;
  const previous = root.getAttribute("style");
  root.style.height = "100vh";
  root.style.overflow = "hidden";
  try {
    await expectScreenshot(story.id, device);
  } finally {
    if (previous === null) root.removeAttribute("style");
    else root.setAttribute("style", previous);
  }
});

async function expectScreenshot(storyId: string, device: string) {
  await expect(page.elementLocator(document.documentElement)).toMatchScreenshot(`${storyId}--${device}`, {
    comparatorName: "pixelmatch",
    comparatorOptions: {
      // Ignore anti-aliasing noise; flag real layout or colour changes.
      threshold: 0.2,
      allowedMismatchedPixelRatio: 0.002,
    },
  });
}
