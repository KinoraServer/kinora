import { cn } from "@/lib/utils";

/**
 * The Kinora mark: a lens ring around a swirl of fanned cards (the drum of
 * photo cards in the original Kinora viewer). Colours come from the theme, so
 * it follows whatever theme is active.
 */
export function Logo({ className }: { className?: string }) {
  const cards = Array.from({ length: 16 }, (_, i) => {
    const a1 = (i * 22.5 * Math.PI) / 180;
    const a2 = ((i * 22.5 + 38) * Math.PI) / 180;
    return {
      i,
      x1: 50 + 7 * Math.cos(a1),
      y1: 50 + 7 * Math.sin(a1),
      x2: 50 + 22 * Math.cos(a2),
      y2: 50 + 22 * Math.sin(a2),
    };
  });
  return (
    <svg
      viewBox="0 0 100 100"
      role="img"
      aria-label="Kinora"
      className={cn("h-8 w-8 shrink-0 text-primary", className)}
    >
      <path
        fillRule="evenodd"
        fill="currentColor"
        d="M10 50a40 40 0 1 0 80 0a40 40 0 1 0-80 0ZM20 50a30 30 0 1 0 60 0a30 30 0 1 0-60 0Z"
      />
      {cards.map((c) => (
        <path
          key={c.i}
          d={`M${c.x1.toFixed(2)} ${c.y1.toFixed(2)}L${c.x2.toFixed(2)} ${c.y2.toFixed(2)}`}
          stroke="currentColor"
          strokeWidth="4.2"
          strokeLinecap="round"
          opacity={0.35 + 0.65 * (c.i / 15)}
        />
      ))}
      <circle cx="50" cy="50" r="5" fill="color-mix(in oklab, currentColor 40%, white)" />
    </svg>
  );
}
