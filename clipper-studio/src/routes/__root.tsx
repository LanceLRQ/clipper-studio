import { createRootRoute, Outlet } from "@tanstack/react-router";
import { TanStackRouterDevtools } from "@tanstack/react-router-devtools";

const RootLayout = () => (
  <>
    <Outlet />
    {import.meta.env.DEV && <TanStackRouterDevtools position="bottom-right" />}
  </>
);

export const Route = createRootRoute({
  component: RootLayout,
});
