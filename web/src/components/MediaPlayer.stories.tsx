import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, waitFor, within } from "storybook/test";
import skinCss from "../../../crates/server/src/modules/skin_example/skin.css?raw";
import skinHtml from "../../../crates/server/src/modules/skin_example/skin.html?raw";
import { demoCues, demoPlayerInfo } from "@/stories/fixtures";
import { PlayerHarness } from "@/stories/PlayerHarness";

/** 42:17 into the 90 minute sample video. */
const START = 42 * 60 + 17;

/** The mocked server setting that picks the player style for every viewer. */
const style = (id: string) => ({ api: { "api/player/style": { style: id } } });

/** The <video> element the player drew. */
const videoOf = (canvasElement: HTMLElement) => {
  const v = canvasElement.ownerDocument.querySelector("video");
  if (!v) throw new Error("no <video> rendered");
  return v;
};

/** Wait until the sample video has loaded and seeked to where the story starts. */
async function videoReady(canvasElement: HTMLElement, at = START) {
  const v = videoOf(canvasElement);
  await waitFor(() => expect(v.readyState >= 2 && Math.abs(v.currentTime - at) < 1).toBe(true), { timeout: 5000 });
  return v;
}

/** The player is `position: fixed`, so query the whole document, not just the story root. */
const screen = (canvasElement: HTMLElement) => within(canvasElement.ownerDocument.body);

/**
 * The video player with every built-in style. The server and the stream are
 * mocked: the video is a local 90 minute clip and the playback info is fixed
 * data, so nothing here needs a running Kinora server.
 *
 * Use the device menu in the toolbar to see each story on phones, tablets,
 * laptops and TVs. Stories tagged `visual` are also screenshotted on a set of
 * devices by `pnpm test:visual` (see docs/storybook.md).
 */
const meta = {
  title: "Player/MediaPlayer",
  component: PlayerHarness,
  parameters: { layout: "fullscreen" },
  args: {
    title: "The Long Drive Home",
    durationHint: 5400,
    startAt: START,
    methodLabel: "Direct play",
    subtitleText: demoCues.short,
    audioOptions: ["English · TrueHD 7.1", "English · AC3 5.1 (Commentary)", "Japanese · AAC 2.0"],
    subtitleOptions: ["English (SRT)", "English SDH (PGS)", "Spanish"],
    getInfo: () => demoPlayerInfo,
    onBack: fn(),
    onPause: fn(),
    onEnded: fn(),
  },
  argTypes: {
    getInfo: { control: false },
    onBack: { control: false },
  },
} satisfies Meta<typeof PlayerHarness>;

export default meta;
type Story = StoryObj<typeof meta>;

// ---------------------------------------------------------------------------
// The four styles
// ---------------------------------------------------------------------------

export const Minimal: Story = {
  tags: ["visual"],
  parameters: style("minimal"),
  play: async ({ canvasElement }) => {
    await videoReady(canvasElement);
    const s = screen(canvasElement);
    await expect(s.getByText("The Long Drive Home")).toBeVisible();
    await expect(s.getByText(demoCues.short)).toBeVisible();
  },
};

export const Cinematic: Story = {
  tags: ["visual"],
  parameters: style("cinematic"),
  play: Minimal.play,
};

export const Floating: Story = {
  tags: ["visual"],
  parameters: style("floating"),
  play: Minimal.play,
};

export const Pro: Story = {
  tags: ["visual"],
  parameters: style("pro"),
  // The Pro bar shows "ends at <clock time>": freeze the clock so screenshots match.
  beforeEach: () => {
    const realNow = Date.now;
    Date.now = () => new Date("2026-01-01T20:00:00").getTime();
    return () => {
      Date.now = realNow;
    };
  },
  play: async (ctx) => {
    await Minimal.play!(ctx);
    await expect(screen(ctx.canvasElement).getByText("Direct play")).toBeVisible();
  },
};

/** A theme can pick the layout; the server setting "theme" follows it. */
export const FollowsThemeLayout: Story = {
  parameters: {
    api: {
      "api/player/style": { style: "theme" },
      "api/themes": {
        default_theme: "ocean",
        themes: [
          {
            id: "ocean",
            name: "Ocean",
            scheme: "dark",
            tokens: { primary: "oklch(0.78 0.13 195)", card: "oklch(0.22 0.035 230)" },
            player: { layout: "floating", options: { "seek-height": "8px", "bar-blur": "20px" } },
          },
        ],
      },
    },
  },
  globals: { theme: "ocean" },
  play: async ({ canvasElement }) => {
    await waitFor(() =>
      expect(canvasElement.ownerDocument.querySelector("[data-player-style]")?.getAttribute("data-player-style")).toBe(
        "floating",
      ),
    );
  },
};

/** The example skin the server writes to player-skins/example-neon.sample. */
export const CustomSkin: Story = {
  parameters: {
    api: {
      "api/player/style": { style: "example-neon" },
      "api/player-skins": [
        { id: "example-neon", name: "Neon (example)", author: "Kinora", html: skinHtml, css: skinCss },
      ],
    },
  },
  play: async ({ canvasElement }) => {
    await waitFor(() =>
      expect(canvasElement.ownerDocument.querySelector("[data-player-style]")?.getAttribute("data-player-style")).toBe(
        "example-neon",
      ),
    );
  },
};

// ---------------------------------------------------------------------------
// Panels and menus
// ---------------------------------------------------------------------------

/** Playback info (the "i" key or the info button), with mocked stats for a 4K HDR transcode. */
export const InfoPanel: Story = {
  tags: ["visual"],
  parameters: style("minimal"),
  args: { methodLabel: "Transcoding" },
  play: async ({ canvasElement }) => {
    await videoReady(canvasElement);
    const s = screen(canvasElement);
    await userEvent.click(s.getByRole("button", { name: "Playback info" }));
    const panel = await s.findByRole("dialog", { name: "Playback info" });
    await expect(within(panel).getByText("HEVC → H.264")).toBeVisible();
    await expect(within(panel).getByText("NVIDIA NVENC")).toBeVisible();
  },
};

/** The info panel's Trace tab, for debugging a stream. */
export const InfoPanelTrace: Story = {
  parameters: style("minimal"),
  play: async (ctx) => {
    await InfoPanel.play!(ctx);
    const s = screen(ctx.canvasElement);
    await userEvent.click(s.getByRole("tab", { name: "Trace" }));
    await expect(s.getByText("Recent segments")).toBeVisible();
    await expect(s.getByRole("button", { name: "Copy all" })).toBeVisible();
    await userEvent.click(s.getByRole("button", { name: "Close playback info" }));
    await expect(s.queryByRole("dialog", { name: "Playback info" })).toBeNull();
  },
};

/** Moving subtitles up and down from the position panel. */
export const SubtitlePosition: Story = {
  tags: ["visual"],
  parameters: style("minimal"),
  args: { subtitleText: demoCues.long },
  play: async ({ canvasElement }) => {
    await videoReady(canvasElement);
    const s = screen(canvasElement);
    await userEvent.click(s.getByRole("button", { name: "Subtitle position" }));
    const panel = await s.findByRole("group", { name: "Subtitle position" });
    const p = within(panel);
    await expect(p.getByText("Default position")).toBeVisible();
    await expect(p.getByRole("button", { name: "Reset subtitle position" })).toBeDisabled();

    await userEvent.click(p.getByRole("button", { name: "Move subtitles up" }));
    await userEvent.click(p.getByRole("button", { name: "Move subtitles up" }));
    await expect(p.getByText(/^Up \d+%$/)).toBeVisible();

    await userEvent.click(p.getByRole("button", { name: "Reset subtitle position" }));
    await expect(p.getByText("Default position")).toBeVisible();

    // While the panel is open, the arrow keys move the subtitles instead of the volume.
    await userEvent.keyboard("{ArrowDown}");
    await expect(p.getByText(/^Down \d+%$/)).toBeVisible();
  },
};

/** Long, styled subtitle lines must wrap inside the screen on every device. */
export const LongSubtitles: Story = {
  tags: ["visual"],
  parameters: style("minimal"),
  args: { subtitleText: `${demoCues.styled}\n${demoCues.long}` },
  play: async ({ canvasElement }) => {
    await videoReady(canvasElement);
    const line = await screen(canvasElement).findByText(/follow the river/);
    const box = line.getBoundingClientRect();
    await expect(box.left).toBeGreaterThanOrEqual(0);
    await expect(box.right).toBeLessThanOrEqual(canvasElement.ownerDocument.documentElement.clientWidth);
  },
};

/** The audio track menu, open. */
export const AudioMenu: Story = {
  tags: ["visual"],
  parameters: style("minimal"),
  play: async ({ canvasElement }) => {
    await videoReady(canvasElement);
    const s = screen(canvasElement);
    await userEvent.click(s.getByRole("button", { name: "Audio track" }));
    const menu = await s.findByRole("menu");
    await expect(within(menu).getAllByRole("menuitemradio")).toHaveLength(3);
    await userEvent.click(within(menu).getByRole("menuitemradio", { name: /Japanese/ }));
    await expect(s.queryByRole("menu")).toBeNull();
  },
};

/** Changing speed from the menu changes the video's playback rate. */
export const SpeedMenu: Story = {
  parameters: style("pro"),
  play: async ({ canvasElement }) => {
    const v = await videoReady(canvasElement);
    const s = screen(canvasElement);
    await userEvent.click(s.getByRole("button", { name: /^Playback speed/ }));
    await userEvent.click(await s.findByRole("menuitemradio", { name: "1.5×" }));
    await expect(v.playbackRate).toBe(1.5);
  },
};

// ---------------------------------------------------------------------------
// Keyboard and TV remote
// ---------------------------------------------------------------------------

/** Every shortcut from the player's controller, checked against the <video> element. */
export const KeyboardShortcuts: Story = {
  parameters: style("minimal"),
  play: async ({ canvasElement, step }) => {
    const v = await videoReady(canvasElement);
    const s = screen(canvasElement);
    // Keys go to the page, not to a focused button.
    (canvasElement.ownerDocument.activeElement as HTMLElement | null)?.blur();

    await step("Arrow keys seek 5 s, J / L seek 10 s", async () => {
      await userEvent.keyboard("{ArrowRight}");
      await waitFor(() => expect(v.currentTime).toBeCloseTo(START + 5, 0));
      await userEvent.keyboard("{ArrowLeft}");
      await waitFor(() => expect(v.currentTime).toBeCloseTo(START, 0));
      await userEvent.keyboard("l");
      await waitFor(() => expect(v.currentTime).toBeCloseTo(START + 10, 0));
      await userEvent.keyboard("j");
      await waitFor(() => expect(v.currentTime).toBeCloseTo(START, 0));
    });

    await step("Up / Down change the volume, M mutes", async () => {
      v.volume = 0.5;
      await userEvent.keyboard("{ArrowUp}");
      await waitFor(() => expect(v.volume).toBeCloseTo(0.6, 5));
      await userEvent.keyboard("{ArrowDown}{ArrowDown}");
      await waitFor(() => expect(v.volume).toBeCloseTo(0.4, 5));
      const muted = v.muted;
      await userEvent.keyboard("m");
      await waitFor(() => expect(v.muted).toBe(!muted));
    });

    await step("I opens the info panel, Escape closes it", async () => {
      await userEvent.keyboard("i");
      await expect(await s.findByRole("dialog", { name: "Playback info" })).toBeVisible();
      await userEvent.keyboard("{Escape}");
      await waitFor(() => expect(s.queryByRole("dialog", { name: "Playback info" })).toBeNull());
    });

    await step("Space and K play and pause", async () => {
      await userEvent.keyboard(" ");
      await waitFor(() => expect(v.paused).toBe(false));
      await userEvent.keyboard("k");
      await waitFor(() => expect(v.paused).toBe(true));
    });
  },
};

// ---------------------------------------------------------------------------
// States
// ---------------------------------------------------------------------------

export const Loading: Story = {
  parameters: style("minimal"),
  args: { loading: true, noVideo: true, subtitleText: undefined },
  play: async ({ canvasElement }) => {
    await expect(await screen(canvasElement).findByRole("status", { name: "Loading" })).toBeVisible();
  },
};

export const PlaybackError: Story = {
  tags: ["visual"],
  parameters: style("minimal"),
  args: {
    noVideo: true,
    subtitleText: undefined,
    error: "The server stopped the transcode: ffmpeg exited with status 1 (No NVIDIA GPU found).",
  },
  play: async ({ canvasElement, args }) => {
    const s = screen(canvasElement);
    await expect(await s.findByText("Couldn't play this")).toBeVisible();
    await userEvent.click(s.getByRole("button", { name: "Go back" }));
    await expect(args.onBack).toHaveBeenCalled();
  },
};

/** A long title and a single audio track and no subtitles (their buttons disappear). */
export const LongTitleNoTracks: Story = {
  tags: ["visual"],
  parameters: style("cinematic"),
  args: {
    title: "Harbour Lights · S03E12 · Crosswind (Part Two of the Extended Director's Cut)",
    audioOptions: ["English · AAC 2.0"],
    subtitleOptions: [],
    subtitleText: undefined,
  },
  play: async ({ canvasElement }) => {
    await videoReady(canvasElement);
    const s = screen(canvasElement);
    await expect(s.queryByRole("button", { name: "Audio track" })).toBeNull();
    await expect(s.queryByRole("button", { name: "Subtitles" })).toBeNull();
  },
};
