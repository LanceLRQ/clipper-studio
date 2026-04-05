import { createFileRoute } from "@tanstack/react-router";

function SettingsPage() {
  return (
    <div className="space-y-4">
      <h2 className="text-2xl font-semibold">设置</h2>
      <p className="text-muted-foreground">
        应用配置、ASR 引擎、插件管理。
      </p>
      <div className="rounded-lg border border-dashed p-12 text-center text-muted-foreground">
        设置功能开发中...
      </div>
    </div>
  );
}

export const Route = createFileRoute("/dashboard/settings")({
  component: SettingsPage,
});
