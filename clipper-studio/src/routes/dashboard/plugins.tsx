import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { PluginInfo, RecorderRoom } from "@/services/plugin";
import {
  scanPlugins,
  listPlugins,
  loadPlugin,
  unloadPlugin,
  startPluginService,
  stopPluginService,
  callPlugin,
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

// ===== Recorder Panel =====
function RecorderPanel({ plugin }: { plugin: PluginInfo }) {
  const [config, setConfig] = useState<Record<string, string>>({
    api_url: "http://127.0.0.1:2007",
    api_key: "",
    webhook_secret: "",
    auto_sync: "false",
  });
  const [status, setStatus] = useState<{
    connected: boolean;
    rooms: RecorderRoom[];
    error: string | null;
  }>({ connected: false, rooms: [], error: null });
  const [loading, setLoading] = useState(false);

  const loadStatus = async () => {
    setLoading(true);
    try {
      const result = await callPlugin(plugin.id, "status", config) as {
        connected: boolean;
        rooms: RecorderRoom[];
      };
      setStatus({ connected: result.connected, rooms: result.rooms, error: null });
    } catch (e) {
      setStatus((prev) => ({ ...prev, connected: false, error: String(e) }));
    } finally {
      setLoading(false);
    }
  };

  const handleConnect = async () => {
    setLoading(true);
    try {
      await callPlugin(plugin.id, "connect", config);
      await loadStatus();
    } catch (e) {
      setStatus((prev) => ({ ...prev, error: String(e) }));
    } finally {
      setLoading(false);
    }
  };

  const handleSync = async () => {
    setLoading(true);
    try {
      await callPlugin(plugin.id, "sync_files", {});
      await loadStatus();
    } catch (e) {
      setStatus((prev) => ({ ...prev, error: String(e) }));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="rounded-lg border p-4 space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="font-medium">{plugin.name} - 录播姬控制台</h3>
        <div className="flex items-center gap-2">
          <span
            className={`text-xs px-2 py-0.5 rounded ${
              status.connected
                ? "bg-green-100 text-green-700"
                : "bg-gray-100 text-gray-500"
            }`}
          >
            {status.connected ? "已连接" : "未连接"}
          </span>
          <Button size="sm" variant="outline" onClick={loadStatus} disabled={loading}>
            刷新
          </Button>
        </div>
      </div>

      {/* Config fields */}
      <div className="grid grid-cols-2 gap-3 text-sm">
        <div>
          <label className="text-xs text-muted-foreground">API 地址</label>
          <Input
            value={config.api_url}
            onChange={(e) =>
              setConfig((c) => ({ ...c, api_url: e.target.value }))
            }
            placeholder="http://127.0.0.1:2007"
          />
        </div>
        <div>
          <label className="text-xs text-muted-foreground">API 密钥</label>
          <Input
            type="password"
            value={config.api_key}
            onChange={(e) =>
              setConfig((c) => ({ ...c, api_key: e.target.value }))
            }
            placeholder="留空则无认证"
          />
        </div>
        <div>
          <label className="text-xs text-muted-foreground">Webhook 密钥</label>
          <Input
            value={config.webhook_secret}
            onChange={(e) =>
              setConfig((c) => ({ ...c, webhook_secret: e.target.value }))
            }
            placeholder="用于验证回调来源"
          />
        </div>
        <div className="flex items-end">
          <Button size="sm" onClick={handleConnect} disabled={loading}>
            {status.connected ? "重新连接" : "连接"}
          </Button>
        </div>
      </div>

      {status.error && (
        <div className="text-xs text-red-500 bg-red-50 rounded p-2">
          {status.error}
        </div>
      )}

      {/* Room list */}
      {status.rooms.length > 0 && (
        <div>
          <div className="flex items-center justify-between mb-2">
            <span className="text-sm font-medium">房间列表</span>
            <Button size="sm" variant="outline" onClick={handleSync} disabled={loading}>
              同步文件
            </Button>
          </div>
          <div className="space-y-1 max-h-48 overflow-y-auto">
            {status.rooms.map((room) => (
              <div
                key={room.roomId}
                className="flex items-center gap-3 text-xs bg-muted/30 rounded p-2"
              >
                <span className="font-medium">{room.name}</span>
                <span className="text-muted-foreground">/ {room.title}</span>
                <div className="ml-auto flex items-center gap-2">
                  {room.recording && (
                    <span className="text-red-500 text-[10px]">录制中</span>
                  )}
                  {room.streaming && (
                    <span className="text-green-500 text-[10px]">直播中</span>
                  )}
                  <span className="text-muted-foreground">
                    {room.areaNameParent} / {room.areaNameChild}
                  </span>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

// ===== Main Plugins Page =====
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

  const recorderPlugins = plugins.filter((p) => p.plugin_type === "Recorder");

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

      {/* Recorder panels */}
      {recorderPlugins.map((plugin) =>
        plugin.status === "loaded" || plugin.status === "running" ? (
          <RecorderPanel key={plugin.id} plugin={plugin} />
        ) : null
      )}

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
