/**
 * shadcn/ui 配色方案预设
 * 基于 Tailwind CSS v4 oklch 色阶，每个方案同时定义浅色和深色变量
 */

export type ThemeColor = "neutral" | "slate" | "zinc" | "stone" | "gray";

export interface ThemePreset {
  label: string;
  description: string;
  light: Record<string, string>;
  dark: Record<string, string>;
}

export const THEME_PRESETS: Record<ThemeColor, ThemePreset> = {
  slate: {
    label: "Slate",
    description: "蓝灰色调，柔和舒适",
    light: {
      "--background": "oklch(1 0 0)",
      "--foreground": "oklch(0.129 0.042 264.695)",
      "--card": "oklch(1 0 0)",
      "--card-foreground": "oklch(0.129 0.042 264.695)",
      "--popover": "oklch(1 0 0)",
      "--popover-foreground": "oklch(0.129 0.042 264.695)",
      "--primary": "oklch(0.208 0.042 265.755)",
      "--primary-foreground": "oklch(0.984 0.003 247.858)",
      "--secondary": "oklch(0.968 0.007 264.542)",
      "--secondary-foreground": "oklch(0.208 0.042 265.755)",
      "--muted": "oklch(0.968 0.007 264.542)",
      "--muted-foreground": "oklch(0.554 0.046 257.417)",
      "--accent": "oklch(0.968 0.007 264.542)",
      "--accent-foreground": "oklch(0.208 0.042 265.755)",
      "--destructive": "oklch(0.577 0.245 27.325)",
      "--border": "oklch(0.929 0.013 255.508)",
      "--input": "oklch(0.929 0.013 255.508)",
      "--ring": "oklch(0.704 0.04 256.788)",
      "--sidebar": "oklch(0.984 0.003 247.858)",
      "--sidebar-foreground": "oklch(0.129 0.042 264.695)",
      "--sidebar-primary": "oklch(0.208 0.042 265.755)",
      "--sidebar-primary-foreground": "oklch(0.984 0.003 247.858)",
      "--sidebar-accent": "oklch(0.968 0.007 264.542)",
      "--sidebar-accent-foreground": "oklch(0.208 0.042 265.755)",
      "--sidebar-border": "oklch(0.929 0.013 255.508)",
      "--sidebar-ring": "oklch(0.704 0.04 256.788)",
    },
    dark: {
      "--background": "oklch(0.129 0.042 264.695)",
      "--foreground": "oklch(0.984 0.003 247.858)",
      "--card": "oklch(0.208 0.042 265.755)",
      "--card-foreground": "oklch(0.984 0.003 247.858)",
      "--popover": "oklch(0.208 0.042 265.755)",
      "--popover-foreground": "oklch(0.984 0.003 247.858)",
      "--primary": "oklch(0.929 0.013 255.508)",
      "--primary-foreground": "oklch(0.208 0.042 265.755)",
      "--secondary": "oklch(0.279 0.041 260.031)",
      "--secondary-foreground": "oklch(0.984 0.003 247.858)",
      "--muted": "oklch(0.279 0.041 260.031)",
      "--muted-foreground": "oklch(0.704 0.04 256.788)",
      "--accent": "oklch(0.279 0.041 260.031)",
      "--accent-foreground": "oklch(0.984 0.003 247.858)",
      "--destructive": "oklch(0.704 0.191 22.216)",
      "--border": "oklch(1 0 0 / 10%)",
      "--input": "oklch(1 0 0 / 15%)",
      "--ring": "oklch(0.554 0.046 257.417)",
      "--sidebar": "oklch(0.208 0.042 265.755)",
      "--sidebar-foreground": "oklch(0.984 0.003 247.858)",
      "--sidebar-primary": "oklch(0.488 0.243 264.376)",
      "--sidebar-primary-foreground": "oklch(0.984 0.003 247.858)",
      "--sidebar-accent": "oklch(0.279 0.041 260.031)",
      "--sidebar-accent-foreground": "oklch(0.984 0.003 247.858)",
      "--sidebar-border": "oklch(1 0 0 / 10%)",
      "--sidebar-ring": "oklch(0.554 0.046 257.417)",
    },
  },
  zinc: {
    label: "Zinc",
    description: "冷灰色调，干净利落",
    light: {
      "--background": "oklch(1 0 0)",
      "--foreground": "oklch(0.141 0.005 285.823)",
      "--card": "oklch(1 0 0)",
      "--card-foreground": "oklch(0.141 0.005 285.823)",
      "--popover": "oklch(1 0 0)",
      "--popover-foreground": "oklch(0.141 0.005 285.823)",
      "--primary": "oklch(0.21 0.006 285.885)",
      "--primary-foreground": "oklch(0.985 0.002 247.839)",
      "--secondary": "oklch(0.967 0.001 286.375)",
      "--secondary-foreground": "oklch(0.21 0.006 285.885)",
      "--muted": "oklch(0.967 0.001 286.375)",
      "--muted-foreground": "oklch(0.552 0.016 285.938)",
      "--accent": "oklch(0.967 0.001 286.375)",
      "--accent-foreground": "oklch(0.21 0.006 285.885)",
      "--destructive": "oklch(0.577 0.245 27.325)",
      "--border": "oklch(0.92 0.004 286.32)",
      "--input": "oklch(0.92 0.004 286.32)",
      "--ring": "oklch(0.705 0.015 286.067)",
      "--sidebar": "oklch(0.985 0.002 247.839)",
      "--sidebar-foreground": "oklch(0.141 0.005 285.823)",
      "--sidebar-primary": "oklch(0.21 0.006 285.885)",
      "--sidebar-primary-foreground": "oklch(0.985 0.002 247.839)",
      "--sidebar-accent": "oklch(0.967 0.001 286.375)",
      "--sidebar-accent-foreground": "oklch(0.21 0.006 285.885)",
      "--sidebar-border": "oklch(0.92 0.004 286.32)",
      "--sidebar-ring": "oklch(0.705 0.015 286.067)",
    },
    dark: {
      "--background": "oklch(0.141 0.005 285.823)",
      "--foreground": "oklch(0.985 0.002 247.839)",
      "--card": "oklch(0.21 0.006 285.885)",
      "--card-foreground": "oklch(0.985 0.002 247.839)",
      "--popover": "oklch(0.21 0.006 285.885)",
      "--popover-foreground": "oklch(0.985 0.002 247.839)",
      "--primary": "oklch(0.92 0.004 286.32)",
      "--primary-foreground": "oklch(0.21 0.006 285.885)",
      "--secondary": "oklch(0.274 0.006 286.033)",
      "--secondary-foreground": "oklch(0.985 0.002 247.839)",
      "--muted": "oklch(0.274 0.006 286.033)",
      "--muted-foreground": "oklch(0.705 0.015 286.067)",
      "--accent": "oklch(0.274 0.006 286.033)",
      "--accent-foreground": "oklch(0.985 0.002 247.839)",
      "--destructive": "oklch(0.704 0.191 22.216)",
      "--border": "oklch(1 0 0 / 10%)",
      "--input": "oklch(1 0 0 / 15%)",
      "--ring": "oklch(0.552 0.016 285.938)",
      "--sidebar": "oklch(0.21 0.006 285.885)",
      "--sidebar-foreground": "oklch(0.985 0.002 247.839)",
      "--sidebar-primary": "oklch(0.488 0.243 264.376)",
      "--sidebar-primary-foreground": "oklch(0.985 0.002 247.839)",
      "--sidebar-accent": "oklch(0.274 0.006 286.033)",
      "--sidebar-accent-foreground": "oklch(0.985 0.002 247.839)",
      "--sidebar-border": "oklch(1 0 0 / 10%)",
      "--sidebar-ring": "oklch(0.552 0.016 285.938)",
    },
  },
  stone: {
    label: "Stone",
    description: "暖灰色调，温润自然",
    light: {
      "--background": "oklch(1 0 0)",
      "--foreground": "oklch(0.147 0.004 49.25)",
      "--card": "oklch(1 0 0)",
      "--card-foreground": "oklch(0.147 0.004 49.25)",
      "--popover": "oklch(1 0 0)",
      "--popover-foreground": "oklch(0.147 0.004 49.25)",
      "--primary": "oklch(0.216 0.006 56.043)",
      "--primary-foreground": "oklch(0.985 0.001 106.423)",
      "--secondary": "oklch(0.97 0.001 106.424)",
      "--secondary-foreground": "oklch(0.216 0.006 56.043)",
      "--muted": "oklch(0.97 0.001 106.424)",
      "--muted-foreground": "oklch(0.553 0.013 58.071)",
      "--accent": "oklch(0.97 0.001 106.424)",
      "--accent-foreground": "oklch(0.216 0.006 56.043)",
      "--destructive": "oklch(0.577 0.245 27.325)",
      "--border": "oklch(0.923 0.003 48.717)",
      "--input": "oklch(0.923 0.003 48.717)",
      "--ring": "oklch(0.709 0.01 56.259)",
      "--sidebar": "oklch(0.985 0.001 106.423)",
      "--sidebar-foreground": "oklch(0.147 0.004 49.25)",
      "--sidebar-primary": "oklch(0.216 0.006 56.043)",
      "--sidebar-primary-foreground": "oklch(0.985 0.001 106.423)",
      "--sidebar-accent": "oklch(0.97 0.001 106.424)",
      "--sidebar-accent-foreground": "oklch(0.216 0.006 56.043)",
      "--sidebar-border": "oklch(0.923 0.003 48.717)",
      "--sidebar-ring": "oklch(0.709 0.01 56.259)",
    },
    dark: {
      "--background": "oklch(0.147 0.004 49.25)",
      "--foreground": "oklch(0.985 0.001 106.423)",
      "--card": "oklch(0.216 0.006 56.043)",
      "--card-foreground": "oklch(0.985 0.001 106.423)",
      "--popover": "oklch(0.216 0.006 56.043)",
      "--popover-foreground": "oklch(0.985 0.001 106.423)",
      "--primary": "oklch(0.923 0.003 48.717)",
      "--primary-foreground": "oklch(0.216 0.006 56.043)",
      "--secondary": "oklch(0.268 0.007 34.298)",
      "--secondary-foreground": "oklch(0.985 0.001 106.423)",
      "--muted": "oklch(0.268 0.007 34.298)",
      "--muted-foreground": "oklch(0.709 0.01 56.259)",
      "--accent": "oklch(0.268 0.007 34.298)",
      "--accent-foreground": "oklch(0.985 0.001 106.423)",
      "--destructive": "oklch(0.704 0.191 22.216)",
      "--border": "oklch(1 0 0 / 10%)",
      "--input": "oklch(1 0 0 / 15%)",
      "--ring": "oklch(0.553 0.013 58.071)",
      "--sidebar": "oklch(0.216 0.006 56.043)",
      "--sidebar-foreground": "oklch(0.985 0.001 106.423)",
      "--sidebar-primary": "oklch(0.488 0.243 264.376)",
      "--sidebar-primary-foreground": "oklch(0.985 0.001 106.423)",
      "--sidebar-accent": "oklch(0.268 0.007 34.298)",
      "--sidebar-accent-foreground": "oklch(0.985 0.001 106.423)",
      "--sidebar-border": "oklch(1 0 0 / 10%)",
      "--sidebar-ring": "oklch(0.553 0.013 58.071)",
    },
  },
  gray: {
    label: "Gray",
    description: "淡蓝灰调，清爽通透",
    light: {
      "--background": "oklch(1 0 0)",
      "--foreground": "oklch(0.13 0.028 261.692)",
      "--card": "oklch(1 0 0)",
      "--card-foreground": "oklch(0.13 0.028 261.692)",
      "--popover": "oklch(1 0 0)",
      "--popover-foreground": "oklch(0.13 0.028 261.692)",
      "--primary": "oklch(0.21 0.034 264.665)",
      "--primary-foreground": "oklch(0.985 0.002 247.839)",
      "--secondary": "oklch(0.968 0.007 264.542)",
      "--secondary-foreground": "oklch(0.21 0.034 264.665)",
      "--muted": "oklch(0.968 0.007 264.542)",
      "--muted-foreground": "oklch(0.551 0.027 264.364)",
      "--accent": "oklch(0.968 0.007 264.542)",
      "--accent-foreground": "oklch(0.21 0.034 264.665)",
      "--destructive": "oklch(0.577 0.245 27.325)",
      "--border": "oklch(0.928 0.006 264.531)",
      "--input": "oklch(0.928 0.006 264.531)",
      "--ring": "oklch(0.707 0.022 261.325)",
      "--sidebar": "oklch(0.985 0.002 247.839)",
      "--sidebar-foreground": "oklch(0.13 0.028 261.692)",
      "--sidebar-primary": "oklch(0.21 0.034 264.665)",
      "--sidebar-primary-foreground": "oklch(0.985 0.002 247.839)",
      "--sidebar-accent": "oklch(0.968 0.007 264.542)",
      "--sidebar-accent-foreground": "oklch(0.21 0.034 264.665)",
      "--sidebar-border": "oklch(0.928 0.006 264.531)",
      "--sidebar-ring": "oklch(0.707 0.022 261.325)",
    },
    dark: {
      "--background": "oklch(0.13 0.028 261.692)",
      "--foreground": "oklch(0.985 0.002 247.839)",
      "--card": "oklch(0.21 0.034 264.665)",
      "--card-foreground": "oklch(0.985 0.002 247.839)",
      "--popover": "oklch(0.21 0.034 264.665)",
      "--popover-foreground": "oklch(0.985 0.002 247.839)",
      "--primary": "oklch(0.928 0.006 264.531)",
      "--primary-foreground": "oklch(0.21 0.034 264.665)",
      "--secondary": "oklch(0.278 0.033 256.848)",
      "--secondary-foreground": "oklch(0.985 0.002 247.839)",
      "--muted": "oklch(0.278 0.033 256.848)",
      "--muted-foreground": "oklch(0.707 0.022 261.325)",
      "--accent": "oklch(0.278 0.033 256.848)",
      "--accent-foreground": "oklch(0.985 0.002 247.839)",
      "--destructive": "oklch(0.704 0.191 22.216)",
      "--border": "oklch(1 0 0 / 10%)",
      "--input": "oklch(1 0 0 / 15%)",
      "--ring": "oklch(0.551 0.027 264.364)",
      "--sidebar": "oklch(0.21 0.034 264.665)",
      "--sidebar-foreground": "oklch(0.985 0.002 247.839)",
      "--sidebar-primary": "oklch(0.488 0.243 264.376)",
      "--sidebar-primary-foreground": "oklch(0.985 0.002 247.839)",
      "--sidebar-accent": "oklch(0.278 0.033 256.848)",
      "--sidebar-accent-foreground": "oklch(0.985 0.002 247.839)",
      "--sidebar-border": "oklch(1 0 0 / 10%)",
      "--sidebar-ring": "oklch(0.551 0.027 264.364)",
    },
  },
  neutral: {
    label: "Neutral",
    description: "纯灰色调，经典硬朗",
    light: {
      "--background": "oklch(1 0 0)",
      "--foreground": "oklch(0.145 0 0)",
      "--card": "oklch(1 0 0)",
      "--card-foreground": "oklch(0.145 0 0)",
      "--popover": "oklch(1 0 0)",
      "--popover-foreground": "oklch(0.145 0 0)",
      "--primary": "oklch(0.205 0 0)",
      "--primary-foreground": "oklch(0.985 0 0)",
      "--secondary": "oklch(0.97 0 0)",
      "--secondary-foreground": "oklch(0.205 0 0)",
      "--muted": "oklch(0.97 0 0)",
      "--muted-foreground": "oklch(0.556 0 0)",
      "--accent": "oklch(0.97 0 0)",
      "--accent-foreground": "oklch(0.205 0 0)",
      "--destructive": "oklch(0.577 0.245 27.325)",
      "--border": "oklch(0.922 0 0)",
      "--input": "oklch(0.922 0 0)",
      "--ring": "oklch(0.708 0 0)",
      "--sidebar": "oklch(0.985 0 0)",
      "--sidebar-foreground": "oklch(0.145 0 0)",
      "--sidebar-primary": "oklch(0.205 0 0)",
      "--sidebar-primary-foreground": "oklch(0.985 0 0)",
      "--sidebar-accent": "oklch(0.97 0 0)",
      "--sidebar-accent-foreground": "oklch(0.205 0 0)",
      "--sidebar-border": "oklch(0.922 0 0)",
      "--sidebar-ring": "oklch(0.708 0 0)",
    },
    dark: {
      "--background": "oklch(0.145 0 0)",
      "--foreground": "oklch(0.985 0 0)",
      "--card": "oklch(0.205 0 0)",
      "--card-foreground": "oklch(0.985 0 0)",
      "--popover": "oklch(0.205 0 0)",
      "--popover-foreground": "oklch(0.985 0 0)",
      "--primary": "oklch(0.922 0 0)",
      "--primary-foreground": "oklch(0.205 0 0)",
      "--secondary": "oklch(0.269 0 0)",
      "--secondary-foreground": "oklch(0.985 0 0)",
      "--muted": "oklch(0.269 0 0)",
      "--muted-foreground": "oklch(0.708 0 0)",
      "--accent": "oklch(0.269 0 0)",
      "--accent-foreground": "oklch(0.985 0 0)",
      "--destructive": "oklch(0.704 0.191 22.216)",
      "--border": "oklch(1 0 0 / 10%)",
      "--input": "oklch(1 0 0 / 15%)",
      "--ring": "oklch(0.556 0 0)",
      "--sidebar": "oklch(0.205 0 0)",
      "--sidebar-foreground": "oklch(0.985 0 0)",
      "--sidebar-primary": "oklch(0.488 0.243 264.376)",
      "--sidebar-primary-foreground": "oklch(0.985 0 0)",
      "--sidebar-accent": "oklch(0.269 0 0)",
      "--sidebar-accent-foreground": "oklch(0.985 0 0)",
      "--sidebar-border": "oklch(1 0 0 / 10%)",
      "--sidebar-ring": "oklch(0.556 0 0)",
    },
  },
};

export const THEME_COLOR_OPTIONS: ThemeColor[] = [
  "slate",
  "zinc",
  "gray",
  "stone",
  "neutral",
];

export const DEFAULT_THEME_COLOR: ThemeColor = "slate";

// ==================== Theme Accent (主题色) ====================

export type ThemeAccent =
  | "default"
  | "blue"
  | "green"
  | "emerald"
  | "orange"
  | "amber"
  | "red"
  | "pink"
  | "purple"
  | "indigo"
  | "cyan"
  | "fuchsia"
  | "lime";

interface AccentOverrides {
  "--primary": string;
  "--primary-foreground": string;
  "--ring": string;
  "--sidebar-primary": string;
  "--sidebar-primary-foreground": string;
  "--sidebar-ring": string;
}

export interface ThemeAccentPreset {
  label: string;
  /** Preview color for the selector dot */
  preview: string;
  light: AccentOverrides;
  dark: AccentOverrides;
}

/** "default" means use base color's built-in primary (no override) */
export const THEME_ACCENT_PRESETS: Record<
  Exclude<ThemeAccent, "default">,
  ThemeAccentPreset
> = {
  blue: {
    label: "Blue",
    preview: "oklch(0.623 0.214 259.815)",
    light: {
      "--primary": "oklch(0.546 0.245 262.881)",
      "--primary-foreground": "oklch(0.97 0.014 254.604)",
      "--ring": "oklch(0.623 0.214 259.815)",
      "--sidebar-primary": "oklch(0.546 0.245 262.881)",
      "--sidebar-primary-foreground": "oklch(0.97 0.014 254.604)",
      "--sidebar-ring": "oklch(0.623 0.214 259.815)",
    },
    dark: {
      "--primary": "oklch(0.707 0.165 254.624)",
      "--primary-foreground": "oklch(0.97 0.014 254.604)",
      "--ring": "oklch(0.707 0.165 254.624)",
      "--sidebar-primary": "oklch(0.707 0.165 254.624)",
      "--sidebar-primary-foreground": "oklch(0.97 0.014 254.604)",
      "--sidebar-ring": "oklch(0.707 0.165 254.624)",
    },
  },
  green: {
    label: "Green",
    preview: "oklch(0.723 0.219 149.579)",
    light: {
      "--primary": "oklch(0.627 0.194 149.214)",
      "--primary-foreground": "oklch(0.982 0.018 155.826)",
      "--ring": "oklch(0.723 0.219 149.579)",
      "--sidebar-primary": "oklch(0.627 0.194 149.214)",
      "--sidebar-primary-foreground": "oklch(0.982 0.018 155.826)",
      "--sidebar-ring": "oklch(0.723 0.219 149.579)",
    },
    dark: {
      "--primary": "oklch(0.792 0.209 151.711)",
      "--primary-foreground": "oklch(0.262 0.051 152.813)",
      "--ring": "oklch(0.792 0.209 151.711)",
      "--sidebar-primary": "oklch(0.792 0.209 151.711)",
      "--sidebar-primary-foreground": "oklch(0.262 0.051 152.813)",
      "--sidebar-ring": "oklch(0.792 0.209 151.711)",
    },
  },
  emerald: {
    label: "Emerald",
    preview: "oklch(0.696 0.17 162.48)",
    light: {
      "--primary": "oklch(0.596 0.145 163.225)",
      "--primary-foreground": "oklch(0.979 0.021 166.113)",
      "--ring": "oklch(0.696 0.17 162.48)",
      "--sidebar-primary": "oklch(0.596 0.145 163.225)",
      "--sidebar-primary-foreground": "oklch(0.979 0.021 166.113)",
      "--sidebar-ring": "oklch(0.696 0.17 162.48)",
    },
    dark: {
      "--primary": "oklch(0.765 0.177 163.223)",
      "--primary-foreground": "oklch(0.262 0.051 152.813)",
      "--ring": "oklch(0.765 0.177 163.223)",
      "--sidebar-primary": "oklch(0.765 0.177 163.223)",
      "--sidebar-primary-foreground": "oklch(0.262 0.051 152.813)",
      "--sidebar-ring": "oklch(0.765 0.177 163.223)",
    },
  },
  orange: {
    label: "Orange",
    preview: "oklch(0.705 0.213 47.604)",
    light: {
      "--primary": "oklch(0.646 0.222 41.116)",
      "--primary-foreground": "oklch(0.98 0.016 73.684)",
      "--ring": "oklch(0.705 0.213 47.604)",
      "--sidebar-primary": "oklch(0.646 0.222 41.116)",
      "--sidebar-primary-foreground": "oklch(0.98 0.016 73.684)",
      "--sidebar-ring": "oklch(0.705 0.213 47.604)",
    },
    dark: {
      "--primary": "oklch(0.779 0.188 70.08)",
      "--primary-foreground": "oklch(0.305 0.064 44.725)",
      "--ring": "oklch(0.779 0.188 70.08)",
      "--sidebar-primary": "oklch(0.779 0.188 70.08)",
      "--sidebar-primary-foreground": "oklch(0.305 0.064 44.725)",
      "--sidebar-ring": "oklch(0.779 0.188 70.08)",
    },
  },
  amber: {
    label: "Amber",
    preview: "oklch(0.769 0.188 70.08)",
    light: {
      "--primary": "oklch(0.666 0.179 58.318)",
      "--primary-foreground": "oklch(0.987 0.022 95.277)",
      "--ring": "oklch(0.769 0.188 70.08)",
      "--sidebar-primary": "oklch(0.666 0.179 58.318)",
      "--sidebar-primary-foreground": "oklch(0.987 0.022 95.277)",
      "--sidebar-ring": "oklch(0.769 0.188 70.08)",
    },
    dark: {
      "--primary": "oklch(0.828 0.189 84.429)",
      "--primary-foreground": "oklch(0.344 0.07 58.601)",
      "--ring": "oklch(0.828 0.189 84.429)",
      "--sidebar-primary": "oklch(0.828 0.189 84.429)",
      "--sidebar-primary-foreground": "oklch(0.344 0.07 58.601)",
      "--sidebar-ring": "oklch(0.828 0.189 84.429)",
    },
  },
  red: {
    label: "Red",
    preview: "oklch(0.637 0.237 25.331)",
    light: {
      "--primary": "oklch(0.577 0.245 27.325)",
      "--primary-foreground": "oklch(0.971 0.013 17.38)",
      "--ring": "oklch(0.637 0.237 25.331)",
      "--sidebar-primary": "oklch(0.577 0.245 27.325)",
      "--sidebar-primary-foreground": "oklch(0.971 0.013 17.38)",
      "--sidebar-ring": "oklch(0.637 0.237 25.331)",
    },
    dark: {
      "--primary": "oklch(0.704 0.191 22.216)",
      "--primary-foreground": "oklch(0.971 0.013 17.38)",
      "--ring": "oklch(0.704 0.191 22.216)",
      "--sidebar-primary": "oklch(0.704 0.191 22.216)",
      "--sidebar-primary-foreground": "oklch(0.971 0.013 17.38)",
      "--sidebar-ring": "oklch(0.704 0.191 22.216)",
    },
  },
  pink: {
    label: "Pink",
    preview: "oklch(0.656 0.241 354.308)",
    light: {
      "--primary": "oklch(0.585 0.22 3.717)",
      "--primary-foreground": "oklch(0.971 0.014 343.198)",
      "--ring": "oklch(0.656 0.241 354.308)",
      "--sidebar-primary": "oklch(0.585 0.22 3.717)",
      "--sidebar-primary-foreground": "oklch(0.971 0.014 343.198)",
      "--sidebar-ring": "oklch(0.656 0.241 354.308)",
    },
    dark: {
      "--primary": "oklch(0.718 0.202 349.761)",
      "--primary-foreground": "oklch(0.971 0.014 343.198)",
      "--ring": "oklch(0.718 0.202 349.761)",
      "--sidebar-primary": "oklch(0.718 0.202 349.761)",
      "--sidebar-primary-foreground": "oklch(0.971 0.014 343.198)",
      "--sidebar-ring": "oklch(0.718 0.202 349.761)",
    },
  },
  purple: {
    label: "Purple",
    preview: "oklch(0.627 0.265 303.9)",
    light: {
      "--primary": "oklch(0.541 0.281 293.009)",
      "--primary-foreground": "oklch(0.969 0.016 293.756)",
      "--ring": "oklch(0.627 0.265 303.9)",
      "--sidebar-primary": "oklch(0.541 0.281 293.009)",
      "--sidebar-primary-foreground": "oklch(0.969 0.016 293.756)",
      "--sidebar-ring": "oklch(0.627 0.265 303.9)",
    },
    dark: {
      "--primary": "oklch(0.702 0.183 293.541)",
      "--primary-foreground": "oklch(0.969 0.016 293.756)",
      "--ring": "oklch(0.702 0.183 293.541)",
      "--sidebar-primary": "oklch(0.702 0.183 293.541)",
      "--sidebar-primary-foreground": "oklch(0.969 0.016 293.756)",
      "--sidebar-ring": "oklch(0.702 0.183 293.541)",
    },
  },
  indigo: {
    label: "Indigo",
    preview: "oklch(0.585 0.233 277.117)",
    light: {
      "--primary": "oklch(0.511 0.262 276.966)",
      "--primary-foreground": "oklch(0.962 0.018 272.314)",
      "--ring": "oklch(0.585 0.233 277.117)",
      "--sidebar-primary": "oklch(0.511 0.262 276.966)",
      "--sidebar-primary-foreground": "oklch(0.962 0.018 272.314)",
      "--sidebar-ring": "oklch(0.585 0.233 277.117)",
    },
    dark: {
      "--primary": "oklch(0.673 0.182 276.935)",
      "--primary-foreground": "oklch(0.962 0.018 272.314)",
      "--ring": "oklch(0.673 0.182 276.935)",
      "--sidebar-primary": "oklch(0.673 0.182 276.935)",
      "--sidebar-primary-foreground": "oklch(0.962 0.018 272.314)",
      "--sidebar-ring": "oklch(0.673 0.182 276.935)",
    },
  },
  cyan: {
    label: "Cyan",
    preview: "oklch(0.715 0.143 215.221)",
    light: {
      "--primary": "oklch(0.609 0.126 221.723)",
      "--primary-foreground": "oklch(0.984 0.019 200.873)",
      "--ring": "oklch(0.715 0.143 215.221)",
      "--sidebar-primary": "oklch(0.609 0.126 221.723)",
      "--sidebar-primary-foreground": "oklch(0.984 0.019 200.873)",
      "--sidebar-ring": "oklch(0.715 0.143 215.221)",
    },
    dark: {
      "--primary": "oklch(0.789 0.154 211.53)",
      "--primary-foreground": "oklch(0.282 0.066 218.204)",
      "--ring": "oklch(0.789 0.154 211.53)",
      "--sidebar-primary": "oklch(0.789 0.154 211.53)",
      "--sidebar-primary-foreground": "oklch(0.282 0.066 218.204)",
      "--sidebar-ring": "oklch(0.789 0.154 211.53)",
    },
  },
  fuchsia: {
    label: "Fuchsia",
    preview: "oklch(0.667 0.295 322.15)",
    light: {
      "--primary": "oklch(0.591 0.293 322.896)",
      "--primary-foreground": "oklch(0.977 0.017 320.058)",
      "--ring": "oklch(0.667 0.295 322.15)",
      "--sidebar-primary": "oklch(0.591 0.293 322.896)",
      "--sidebar-primary-foreground": "oklch(0.977 0.017 320.058)",
      "--sidebar-ring": "oklch(0.667 0.295 322.15)",
    },
    dark: {
      "--primary": "oklch(0.74 0.238 322.16)",
      "--primary-foreground": "oklch(0.977 0.017 320.058)",
      "--ring": "oklch(0.74 0.238 322.16)",
      "--sidebar-primary": "oklch(0.74 0.238 322.16)",
      "--sidebar-primary-foreground": "oklch(0.977 0.017 320.058)",
      "--sidebar-ring": "oklch(0.74 0.238 322.16)",
    },
  },
  lime: {
    label: "Lime",
    preview: "oklch(0.768 0.233 130.85)",
    light: {
      "--primary": "oklch(0.648 0.2 131.684)",
      "--primary-foreground": "oklch(0.986 0.031 120.757)",
      "--ring": "oklch(0.768 0.233 130.85)",
      "--sidebar-primary": "oklch(0.648 0.2 131.684)",
      "--sidebar-primary-foreground": "oklch(0.986 0.031 120.757)",
      "--sidebar-ring": "oklch(0.768 0.233 130.85)",
    },
    dark: {
      "--primary": "oklch(0.841 0.238 128.85)",
      "--primary-foreground": "oklch(0.312 0.082 131.327)",
      "--ring": "oklch(0.841 0.238 128.85)",
      "--sidebar-primary": "oklch(0.841 0.238 128.85)",
      "--sidebar-primary-foreground": "oklch(0.312 0.082 131.327)",
      "--sidebar-ring": "oklch(0.841 0.238 128.85)",
    },
  },
};

export const THEME_ACCENT_OPTIONS: ThemeAccent[] = [
  "default",
  "blue",
  "green",
  "emerald",
  "orange",
  "amber",
  "red",
  "pink",
  "purple",
  "indigo",
  "cyan",
  "fuchsia",
  "lime",
];

export const DEFAULT_THEME_ACCENT: ThemeAccent = "default";

// ==================== Apply Functions ====================

/** Apply base color preset CSS variables to the document root */
export function applyColorPreset(
  color: ThemeColor,
  resolvedTheme: "light" | "dark"
) {
  const preset = THEME_PRESETS[color];
  if (!preset) return;

  const vars = resolvedTheme === "dark" ? preset.dark : preset.light;
  const root = document.documentElement;
  for (const [key, value] of Object.entries(vars)) {
    root.style.setProperty(key, value);
  }
}

/** Apply accent color overrides on top of base color */
export function applyAccentPreset(
  accent: ThemeAccent,
  resolvedTheme: "light" | "dark"
) {
  if (accent === "default") return; // Use base color's built-in primary

  const preset = THEME_ACCENT_PRESETS[accent];
  if (!preset) return;

  const vars = resolvedTheme === "dark" ? preset.dark : preset.light;
  const root = document.documentElement;
  for (const [key, value] of Object.entries(vars)) {
    root.style.setProperty(key, value);
  }
}
