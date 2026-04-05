import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";

interface AppInfo {
  version: string;
  data_dir: string;
  ffmpeg_available: boolean;
  ffmpeg_version: string | null;
  ffprobe_available: boolean;
}

function App() {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    invoke<AppInfo>("get_app_info")
      .then(setAppInfo)
      .catch(console.error)
      .finally(() => setLoading(false));
  }, []);

  if (loading) {
    return (
      <div className="flex h-screen items-center justify-center">
        <div className="text-muted-foreground">Loading...</div>
      </div>
    );
  }

  return (
    <div className="flex h-screen flex-col">
      {/* Header */}
      <header className="flex h-14 items-center border-b px-6">
        <h1 className="text-lg font-semibold">ClipperStudio</h1>
        <span className="ml-2 text-xs text-muted-foreground">
          v{appInfo?.version}
        </span>
      </header>

      {/* Main Content */}
      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar */}
        <aside className="w-56 border-r p-4">
          <nav className="space-y-1">
            <Button variant="ghost" className="w-full justify-start">
              视频列表
            </Button>
            <Button variant="ghost" className="w-full justify-start">
              任务中心
            </Button>
            <Button variant="ghost" className="w-full justify-start">
              设置
            </Button>
          </nav>
        </aside>

        {/* Content Area */}
        <main className="flex-1 overflow-auto p-6">
          <div className="mx-auto max-w-3xl space-y-6">
            <h2 className="text-2xl font-semibold">欢迎使用 ClipperStudio</h2>
            <p className="text-muted-foreground">
              本地优先的桌面视频工作台，面向直播录播切片创作者。
            </p>

            {/* System Status */}
            <div className="rounded-lg border p-4 space-y-3">
              <h3 className="font-medium">系统状态</h3>

              <div className="grid grid-cols-2 gap-4 text-sm">
                <div>
                  <span className="text-muted-foreground">数据目录：</span>
                  <span className="break-all">{appInfo?.data_dir}</span>
                </div>

                <div>
                  <span className="text-muted-foreground">FFmpeg：</span>
                  {appInfo?.ffmpeg_available ? (
                    <span className="text-green-600">
                      ✓ {appInfo.ffmpeg_version?.split(" ").slice(0, 3).join(" ")}
                    </span>
                  ) : (
                    <span className="text-red-500">✗ 未检测到</span>
                  )}
                </div>

                <div>
                  <span className="text-muted-foreground">FFprobe：</span>
                  {appInfo?.ffprobe_available ? (
                    <span className="text-green-600">✓ 可用</span>
                  ) : (
                    <span className="text-red-500">✗ 未检测到</span>
                  )}
                </div>
              </div>
            </div>
          </div>
        </main>
      </div>
    </div>
  );
}

export default App;
