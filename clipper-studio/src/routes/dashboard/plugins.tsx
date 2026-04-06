import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import type { PluginInfo } from "@/services/plugin";
import {
  scanPlugins,
  listPlugins,
  loadPlugin,
  unloadPlugin,
  startPluginService,
  stopPluginService,
} from "@/services/plugin";

const STATUS_LABELS: Record<string, { label: string; color: string }> = {
  discovered: { label: "已发现", color: "bg-gray-100 text-gray-700" },
  loaded: { label: "已加载", color: "bg-blue-100 text-blue-700" },
  running: { label: "运行中", color: "bg-green-100 text-green-700" },
  error: { label: "错误", color: "bg-red-100 text-red-700" },
  disabled: { label: "已禁用", color: "bg-yellow-100 text-yellow-700" },
  incompatible: { label: "不兼容", color: "bg-red-100 text-red-700" },
};

const TYPE_LABELS: Record<string, string> = {
  AsrEngine: "ASR 引擎",
  LlmProvider: "LLM 提供者",
  Recorder: "录制工具",
  Uploader: "上传工具",
  SyncProvider: "同步服务",
  WorkspaceAdapter: "工作区适配器",
  DanmakuSource: "弹幕源",
  DanmakuRenderer: "弹幕渲染",
  Exporter: "导出工具",
  StorageProvider: "存储提供者",
};

function PluginsPage() {
  const [plugins, setPlugins] = useState<PluginInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState<string | null>(null);

  const loadData = async () => {
    setLoading(true);
    try {
      const list = await listPlugins();
      setPlugins(list);
    } catch (e) {
      console.error("Failed to load plugins:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadData();
  }, []);

  const handleScan = async () => {
    setLoading(true);
    try {
      const list = await scanPlugins();
      setPlugins(list);
    } catch (e) {
      console.error("Scan failed:", e);
    } finally {
      setLoading(false);
    }
  };

  const handleAction = async (
    pluginId: string,
    action: "load" | "unload" | "start" | "stop"
  ) => {
    setActionLoading(pluginId);
    try {
      switch (action) {
        case "load":
          await loadPlugin(pluginId);
          break;
        case "unload":
          await unloadPlugin(pluginId);
          break;
        case "start":
          await startPluginService(pluginId);
          break;
        case "stop":
          await stopPluginService(pluginId);
          break;
      }
      await loadData();
    } catch (e) {
      alert(`操作失败: ${String(e)}`);
    } finally {
      setActionLoading(null);
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-semibold">插件管理</h2>
          <p className="text-sm text-muted-foreground">
            {plugins.length > 0
              ? `已发现 ${plugins.length} 个插件`
              : "暂无插件"}
          </p>
        </div>
        <Button onClick={handleScan} disabled={loading}>
          {loading ? "扫描中..." : "扫描插件"}
        </Button>
      </div>

      {loading && plugins.length === 0 ? (
        <div className="text-muted-foreground">加载中...</div>
      ) : plugins.length === 0 ? (
        <div className="rounded-lg border border-dashed p-12 text-center">
          <p className="text-muted-foreground mb-2">暂无已安装的插件</p>
          <p className="text-xs text-muted-foreground">
            将插件目录放入应用数据目录的 plugins/ 文件夹，然后点击"扫描插件"
          </p>
        </div>
      ) : (
        <div className="space-y-3">
          {plugins.map((plugin) => {
            const statusInfo = STATUS_LABELS[plugin.status] ?? {
              label: plugin.status,
              color: "bg-gray-100 text-gray-700",
            };
            const typeLabel =
              TYPE_LABELS[plugin.plugin_type] ?? plugin.plugin_type;
            const isLoading = actionLoading === plugin.id;

            return (
              <div
                key={plugin.id}
                className="rounded-lg border p-4 space-y-2"
              >
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-3">
                    <div>
                      <div className="flex items-center gap-2">
                        <span className="font-medium">{plugin.name}</span>
                        <span className="text-xs text-muted-foreground">
                          v{plugin.version}
                        </span>
                        <span
                          className={`text-xs px-1.5 py-0.5 rounded ${statusInfo.color}`}
                        >
                          {statusInfo.label}
                        </span>
                      </div>
                      <div className="flex gap-3 text-xs text-muted-foreground mt-0.5">
                        <span>{typeLabel}</span>
                        <span>
                          {plugin.transport === "Http" ? "HTTP" : "Stdio"}
                        </span>
                        {plugin.managed && <span>托管进程</span>}
                      </div>
                    </div>
                  </div>

                  {/* Action buttons */}
                  <div className="flex gap-2">
                    {plugin.status === "discovered" && (
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={isLoading}
                        onClick={() => handleAction(plugin.id, "load")}
                      >
                        加载
                      </Button>
                    )}
                    {plugin.status === "loaded" && (
                      <>
                        {plugin.managed && (
                          <Button
                            size="sm"
                            disabled={isLoading}
                            onClick={() => handleAction(plugin.id, "start")}
                          >
                            启动
                          </Button>
                        )}
                        <Button
                          size="sm"
                          variant="outline"
                          disabled={isLoading}
                          onClick={() => handleAction(plugin.id, "unload")}
                        >
                          卸载
                        </Button>
                      </>
                    )}
                    {plugin.status === "running" && (
                      <Button
                        size="sm"
                        variant="destructive"
                        disabled={isLoading}
                        onClick={() => handleAction(plugin.id, "stop")}
                      >
                        停止
                      </Button>
                    )}
                  </div>
                </div>

                {plugin.description && (
                  <p className="text-xs text-muted-foreground">
                    {plugin.description}
                  </p>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

export const Route = createFileRoute("/dashboard/plugins")({
  component: PluginsPage,
});
