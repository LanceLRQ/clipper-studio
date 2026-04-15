import { createRootRoute, Outlet } from "@tanstack/react-router";
import { TanStackRouterDevtools } from "@tanstack/react-router-devtools";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { useThemeStore } from "@/stores/theme";
import { useWorkspaceStore } from "@/stores/workspace";

const RootLayout = () => {
  const initTheme = useThemeStore((s) => s.initialize);
  const initWorkspace = useWorkspaceStore((s) => s.initialize);
  const [debugMode, setDebugMode] = useState(false);

  useEffect(() => {
    initTheme();
    initWorkspace();
    invoke<boolean>("is_debug_mode")
      .then(setDebugMode)
      .catch(() => setDebugMode(false));
  }, [initTheme, initWorkspace]);

  return (
    <>
      <Outlet />
      {debugMode && <TanStackRouterDevtools position="bottom-right" />}
    </>
  );
};

export const Route = createRootRoute({
  component: RootLayout,
});
