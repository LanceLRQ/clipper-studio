/** Color palette for multi-clip regions (Tailwind-aligned) */
export const CLIP_COLORS = [
  "#3b82f6", // blue-500
  "#22c55e", // green-500
  "#eab308", // yellow-500
  "#ec4899", // pink-500
  "#8b5cf6", // violet-500
  "#06b6d4", // cyan-500
  "#f97316", // orange-500
];

/** Maximum number of clips allowed */
export const MAX_CLIPS = 10;

/** Get color for a clip by index */
export function getClipColor(index: number): string {
  return CLIP_COLORS[index % CLIP_COLORS.length];
}
