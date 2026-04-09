import { create } from "zustand";
import { getSetting, setSetting } from "@/services/settings";
import {
  type ThemeColor,
  type ThemeAccent,
  DEFAULT_THEME_COLOR,
  DEFAULT_THEME_ACCENT,
  THEME_COLOR_OPTIONS,
  THEME_ACCENT_OPTIONS,
  applyColorPreset,
  applyAccentPreset,
} from "@/lib/theme-presets";

export type ThemeMode = "light" | "dark" | "system";
type ResolvedTheme = "light" | "dark";

const MODE_STORAGE_KEY = "clipper-theme";
const COLOR_STORAGE_KEY = "clipper-theme-color";
const ACCENT_STORAGE_KEY = "clipper-theme-accent";

interface ThemeState {
  mode: ThemeMode;
  colorScheme: ThemeColor;
  accent: ThemeAccent;
  resolved: ResolvedTheme;
  initialized: boolean;
  initialize: () => Promise<void>;
  setMode: (mode: ThemeMode) => void;
  setColorScheme: (color: ThemeColor) => void;
  setAccent: (accent: ThemeAccent) => void;
}

function resolveTheme(mode: ThemeMode): ResolvedTheme {
  if (mode === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
  }
  return mode;
}

function applyTheme(resolved: ResolvedTheme) {
  if (resolved === "dark") {
    document.documentElement.classList.add("dark");
  } else {
    document.documentElement.classList.remove("dark");
  }
}

/** Apply dark/light class + base color + accent overrides */
function applyFullTheme(
  resolved: ResolvedTheme,
  color: ThemeColor,
  accent: ThemeAccent
) {
  applyTheme(resolved);
  applyColorPreset(color, resolved);
  applyAccentPreset(accent, resolved); // Must be after base color (overrides primary)
}

export const useThemeStore = create<ThemeState>((set, get) => {
  // 监听系统主题变化
  const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
  mediaQuery.addEventListener("change", () => {
    const { mode, colorScheme, accent } = get();
    if (mode === "system") {
      const resolved = resolveTheme("system");
      applyFullTheme(resolved, colorScheme, accent);
      set({ resolved });
    }
  });

  return {
    mode: "system",
    colorScheme: DEFAULT_THEME_COLOR,
    accent: DEFAULT_THEME_ACCENT,
    resolved: resolveTheme("system"),
    initialized: false,

    async initialize() {
      if (get().initialized) return;

      try {
        const [savedMode, savedColor, savedAccent] = await Promise.all([
          getSetting("theme"),
          getSetting("theme_color"),
          getSetting("theme_accent"),
        ]);

        const mode = (
          savedMode && ["light", "dark", "system"].includes(savedMode)
            ? savedMode
            : "system"
        ) as ThemeMode;

        const colorScheme = (
          savedColor &&
          THEME_COLOR_OPTIONS.includes(savedColor as ThemeColor)
            ? savedColor
            : DEFAULT_THEME_COLOR
        ) as ThemeColor;

        const accent = (
          savedAccent &&
          THEME_ACCENT_OPTIONS.includes(savedAccent as ThemeAccent)
            ? savedAccent
            : DEFAULT_THEME_ACCENT
        ) as ThemeAccent;

        const resolved = resolveTheme(mode);

        applyFullTheme(resolved, colorScheme, accent);
        localStorage.setItem(MODE_STORAGE_KEY, mode);
        localStorage.setItem(COLOR_STORAGE_KEY, colorScheme);
        localStorage.setItem(ACCENT_STORAGE_KEY, accent);
        set({ mode, colorScheme, accent, resolved, initialized: true });
      } catch {
        const resolved = resolveTheme("system");
        applyFullTheme(resolved, DEFAULT_THEME_COLOR, DEFAULT_THEME_ACCENT);
        set({
          mode: "system",
          colorScheme: DEFAULT_THEME_COLOR,
          accent: DEFAULT_THEME_ACCENT,
          resolved,
          initialized: true,
        });
      }
    },

    setMode(mode: ThemeMode) {
      const { colorScheme, accent } = get();
      const resolved = resolveTheme(mode);
      applyFullTheme(resolved, colorScheme, accent);

      localStorage.setItem(MODE_STORAGE_KEY, mode);
      setSetting("theme", mode).catch(() => {});

      set({ mode, resolved });
    },

    setColorScheme(color: ThemeColor) {
      const { resolved, accent } = get();
      applyColorPreset(color, resolved);
      applyAccentPreset(accent, resolved); // Re-apply accent on top

      localStorage.setItem(COLOR_STORAGE_KEY, color);
      setSetting("theme_color", color).catch(() => {});

      set({ colorScheme: color });
    },

    setAccent(accent: ThemeAccent) {
      const { resolved, colorScheme } = get();
      // Re-apply base first to reset primary, then overlay accent
      applyColorPreset(colorScheme, resolved);
      applyAccentPreset(accent, resolved);

      localStorage.setItem(ACCENT_STORAGE_KEY, accent);
      setSetting("theme_accent", accent).catch(() => {});

      set({ accent });
    },
  };
});
