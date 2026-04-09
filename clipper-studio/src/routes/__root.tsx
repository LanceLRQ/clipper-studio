import { createRootRoute, Outlet } from "@tanstack/react-router";
import { TanStackRouterDevtools } from "@tanstack/react-router-devtools";
import { useEffect } from "react";
import { useThemeStore } from "@/stores/theme";
import { useWorkspaceStore } from "@/stores/workspace";

const RootLayout = () => {
  const initTheme = useThemeStore((s) => s.initialize);
  const initWorkspace = useWorkspaceStore((s) => s.initialize);

  useEffect(() => {
    initTheme();
    initWorkspace();
  }, [initTheme, initWorkspace]);

  return (
    <>
      <Outlet />
      {import.meta.env.DEV && <TanStackRouterDevtools position="bottom-right" />}
    </>
  );
};

export const Route = createRootRoute({
  component: RootLayout,
});
