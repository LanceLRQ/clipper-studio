import { createFileRoute, Link } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import type { PluginInfo, RecorderRoom } from "@/services/plugin";
import {
  listPlugins,
  callPlugin,
  getPluginConfig,
} from "@/services/plugin";

// ===== Recorder Panel =====
function RecorderPanel({ plugin }: { plugin: PluginInfo }) {
  const [status, setStatus] = useState<{
    connected: boolean;
    rooms: RecorderRoom[];
    error: string | null;
  }>({ connected: false, rooms: [], error: null });
  const [loading, setLoading] = useState(false);

  const buildPayload = async () => {
    const cfg = await getPluginConfig(plugin.id);
    const payload: Record<string, string> = {
      base_url: cfg.api_url || "http://127.0.0.1:2007",
    };
    if (cfg.api_key) payload.api_key = cfg.api_key;
    if (cfg.basic_user && cfg.basic_pass) {
      payload.basic_user = cfg.basic_user;
      payload.basic_pass = cfg.basic_pass;
    }
    return payload;
  };

  const loadStatus = async () => {
    setLoading(true);
    try {
      const payload = await buildPayload();
      const result = (await callPlugin(plugin.id, "status", payload)) as {
        connected: boolean;
        rooms: RecorderRoom[];
      };
      setStatus({
        connected: result.connected,
        rooms: result.rooms ?? [],
        error: null,
      });
    } catch (e) {
      setStatus((prev) => ({ ...prev, connected: false, error: String(e) }));
    } finally {
      setLoading(false);
    }
  };

  const handleSync = async () => {
    setLoading(true);
    try {
      const payload = await buildPayload();
      await callPlugin(plugin.id, "sync_files", payload);
      await loadStatus();
    } catch (e) {
      setStatus((prev) => ({ ...prev, error: String(e) }));
    } finally {
      setLoading(false);
    }
  };

  // Auto-load status on mount
  useEffect(() => {
    loadStatus();
  }, [plugin.id]);

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
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
        </div>
        <div className="flex items-center gap-2">
          <Button
            size="sm"
            variant="outline"
            onClick={handleSync}
            disabled={loading}
          >
            同步文件
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={loadStatus}
            disabled={loading}
          >
            刷新状态
          </Button>
        </div>
      </div>

      <p className="text-xs text-muted-foreground">
        录播姬地址和认证在「设置」页面中配置
      </p>

      {status.error && (
        <div className="text-xs text-red-500 bg-red-50 rounded p-2">
          {status.error}
        </div>
      )}

      {/* Room list */}
      {status.rooms.length > 0 && (
        <div>
          <div className="flex items-center justify-between mb-2">
            <span className="text-sm font-medium">
              房间列表（{status.rooms.length}）
            </span>
          </div>
          <div className="space-y-1 max-h-[calc(100vh-300px)] overflow-y-auto">
            {status.rooms.map((room) => (
              <div
                key={room.roomId}
                className="flex items-center gap-3 text-xs bg-muted/30 rounded p-2"
              >
                <span className="font-medium">{room.name}</span>
                <span className="text-muted-foreground truncate max-w-[200px]">
                  / {room.title}
                </span>
                <div className="ml-auto flex items-center gap-2 shrink-0">
                  {room.recording && (
                    <span className="text-red-500 text-[10px]">录制中</span>
                  )}
                  {room.streaming && (
                    <span className="text-green-500 text-[10px]">直播中</span>
                  )}
                  <span className="text-muted-foreground text-[10px]">
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

// ===== Generic Plugin Panel =====
function GenericPluginPanel({ plugin }: { plugin: PluginInfo }) {
  return (
    <div className="space-y-4">
      <div className="rounded-lg border p-4 space-y-2">
        <div className="grid grid-cols-2 gap-2 text-sm">
          <span className="text-muted-foreground">插件 ID</span>
          <span className="font-mono text-xs">{plugin.id}</span>
          <span className="text-muted-foreground">版本</span>
          <span>{plugin.version}</span>
          <span className="text-muted-foreground">类型</span>
          <span>{plugin.plugin_type}</span>
          <span className="text-muted-foreground">传输方式</span>
          <span>{plugin.transport}</span>
          <span className="text-muted-foreground">状态</span>
          <span>{plugin.status}</span>
        </div>
      </div>
      {plugin.description && (
        <p className="text-sm text-muted-foreground">{plugin.description}</p>
      )}
    </div>
  );
}

// ===== Plugin Detail Page =====
function PluginDetailPage() {
  const { pluginId } = Route.useParams();
  const [plugin, setPlugin] = useState<PluginInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const load = async () => {
      setLoading(true);
      try {
        const plugins = await listPlugins();
        const found = plugins.find((p) => p.id === pluginId);
        if (found) {
          setPlugin(found);
        } else {
          setError(`插件 "${pluginId}" 未找到`);
        }
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
    };
    load();
  }, [pluginId]);

  if (loading) {
    return <div className="text-muted-foreground">加载中...</div>;
  }

  if (error || !plugin) {
    return (
      <div className="space-y-4">
        <Link to="/dashboard/plugins">
          <Button variant="ghost" size="sm">
            &larr; 返回插件管理
          </Button>
        </Link>
        <div className="text-red-500">{error ?? "插件未找到"}</div>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center gap-3">
        <Link to="/dashboard/plugins">
          <Button variant="ghost" size="sm">
            &larr;
          </Button>
        </Link>
        <div>
          <h2 className="text-2xl font-semibold">{plugin.name}</h2>
          <p className="text-sm text-muted-foreground">
            v{plugin.version} &middot; {plugin.plugin_type}
          </p>
        </div>
      </div>

      {/* Plugin-specific panel */}
      {plugin.plugin_type === "Recorder" ? (
        <RecorderPanel plugin={plugin} />
      ) : (
        <GenericPluginPanel plugin={plugin} />
      )}
    </div>
  );
}

export const Route = createFileRoute("/dashboard/plugins/$pluginId")({
  component: PluginDetailPage,
});
