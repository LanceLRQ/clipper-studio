import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useEffect } from "react";

/**
 * Legacy workspace management route — redirects to settings page.
 * Kept to avoid broken links / bookmarks.
 */
function WorkspacesRedirect() {
  const navigate = useNavigate();

  useEffect(() => {
    navigate({
      to: "/dashboard/settings",
      search: { section: "workspaces" },
      replace: true,
    });
  }, [navigate]);

  return null;
}

export const Route = createFileRoute("/dashboard/workspaces")({
  component: WorkspacesRedirect,
});
