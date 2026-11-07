// Screen sizes Kinora has to look right on, in CSS pixels (what the browser
// reports, not the panel's physical pixels). One list feeds both the
// Storybook toolbar (preview.tsx) and the visual tests (vitest.setup.ts), so
// adding a device here adds it everywhere.
//
// TVs: most TV browsers and Android TV WebViews render a 1080p panel at
// 960×540 CSS px (device pixel ratio 2), and a 4K panel at 1920×1080 (DPR 2).
// The "TV 4K native" entry covers the rarer browsers that run at DPR 1.

export type DeviceKind = "phone" | "tablet" | "desktop" | "tv";

export interface Device {
  name: string;
  kind: DeviceKind;
  width: number;
  height: number;
  /** Include in the visual test run (vitest.setup.ts). The rest are toolbar-only. */
  visual?: boolean;
}

export const DEVICES = {
  // Phones, portrait
  phoneSmall: { name: "Small Android (360×800)", kind: "phone", width: 360, height: 800, visual: true },
  iphoneSE: { name: "iPhone SE (375×667)", kind: "phone", width: 375, height: 667 },
  iphonePro: { name: "iPhone 16 Pro (402×874)", kind: "phone", width: 402, height: 874 },
  pixel: { name: "Pixel 10 (412×915)", kind: "phone", width: 412, height: 915, visual: true },
  // Phones, landscape (short screens are the hard case for the player)
  phoneLandscapeShort: {
    name: "iPhone SE landscape (667×375)",
    kind: "phone",
    width: 667,
    height: 375,
    visual: true,
  },
  pixelLandscape: { name: "Pixel 10 landscape (915×412)", kind: "phone", width: 915, height: 412 },
  // Tablets
  ipadMini: { name: "iPad mini (744×1133)", kind: "tablet", width: 744, height: 1133 },
  ipadPro: { name: "iPad Pro 11 (834×1210)", kind: "tablet", width: 834, height: 1210, visual: true },
  ipadProLandscape: { name: "iPad Pro 11 landscape (1210×834)", kind: "tablet", width: 1210, height: 834 },
  // Computers
  laptop: { name: "Laptop (1280×800)", kind: "desktop", width: 1280, height: 800, visual: true },
  macbookPro: { name: "MacBook Pro 14 (1512×982)", kind: "desktop", width: 1512, height: 982 },
  desktop: { name: "Desktop 1080p (1920×1080)", kind: "desktop", width: 1920, height: 1080, visual: true },
  ultrawide: { name: "Ultrawide (2560×1080)", kind: "desktop", width: 2560, height: 1080 },
  // TVs
  tv1080: { name: "TV 1080p (960×540)", kind: "tv", width: 960, height: 540, visual: true },
  tv4k: { name: "TV 4K (1920×1080)", kind: "tv", width: 1920, height: 1080 },
  tv4kNative: { name: "TV 4K native (3840×2160)", kind: "tv", width: 3840, height: 2160, visual: true },
} satisfies Record<string, Device>;

export type DeviceId = keyof typeof DEVICES;

const TOOLBAR_TYPE: Record<DeviceKind, "mobile" | "tablet" | "desktop" | "other"> = {
  phone: "mobile",
  tablet: "tablet",
  desktop: "desktop",
  tv: "other",
};

/** The shape Storybook's viewport toolbar wants (`parameters.viewport.options`). */
export const VIEWPORT_OPTIONS = Object.fromEntries(
  Object.entries(DEVICES).map(([id, d]) => [
    id,
    { name: d.name, type: TOOLBAR_TYPE[d.kind], styles: { width: `${d.width}px`, height: `${d.height}px` } },
  ]),
);

/** Devices the visual tests screenshot every `visual` story on. */
export const VISUAL_DEVICES = (Object.entries(DEVICES) as [DeviceId, Device][]).filter(([, d]) => d.visual);
