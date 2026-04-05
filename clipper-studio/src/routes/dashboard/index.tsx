import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface AppInfo {
  version: string;
  data_dir: string;
  ffmpeg_available: boolean;
  ffmpeg_version: string | null;
  ffprobe_available: boolean;
}

function DashboardIndex() {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    invoke<AppInfo>("get_app_info")
      .then(setAppInfo)
      .catch(console.error);
  }, []);

  return (
    <div className="mx-auto max-w-3xl space-y-6">
      <h2 className="text-2xl font-semibold">欢迎使用 ClipperStudio</h2>
      <p className="text-muted-foreground">
        本地优先的桌面视频工作台，面向直播录播切片创作者。
      </p>

      {/* System Status */}
      <div className="rounded-lg border p-4 space-y-3">
        <h3 className="font-medium">系统状态</h3>

        {appInfo ? (
          <div className="grid grid-cols-2 gap-4 text-sm">
            <div>
              <span className="text-muted-foreground">数据目录：</span>
              <span className="break-all">{appInfo.data_dir}</span>
            </div>
            <div>
              <span className="text-muted-foreground">FFmpeg：</span>
              {appInfo.ffmpeg_available ? (
                <span className="text-green-600">
                  ✓{" "}
                  {appInfo.ffmpeg_version
                    ?.split(" ")
                    .slice(0, 3)
                    .join(" ")}
                </span>
              ) : (
                <span className="text-red-500">✗ 未检测到</span>
              )}
            </div>
            <div>
              <span className="text-muted-foreground">FFprobe：</span>
              {appInfo.ffprobe_available ? (
                <span className="text-green-600">✓ 可用</span>
              ) : (
                <span className="text-red-500">✗ 未检测到</span>
              )}
            </div>
          </div>
        ) : (
          <div className="text-sm text-muted-foreground">加载中...</div>
        )}
      </div>
    </div>
  );
}

export const Route = createFileRoute("/dashboard/")({
  component: DashboardIndex,
});
