import { useState, useEffect, useCallback } from "react";
import {
  HardDriveIcon,
  PlugIcon,
  UnplugIcon,
  Loader2Icon,
  FolderPlusIcon,
  AlertTriangleIcon,
} from "lucide-react";
import { ask } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

const PLUGIN_ID = "builtin.storage.smb";

interface MountInfo {
  server: string;
  share: string;
  mount_point: string;
}

interface PlatformCheck {
  supported: boolean;
  platform: string;
}

export function StorageManager() {
  const [mounts, setMounts] = useState<MountInfo[]>([]);
  const [check, setCheck] = useState<PlatformCheck | null>(null);
  const [loading, setLoading] = useState(true);
  const [mounting, setMounting] = useState(false);
  const [error, setError] = useState("");

  // Form state
  const [server, setServer] = useState("");
  const [share, setShare] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [mountPoint, setMountPoint] = useState("");

  const callPlugin = useCallback(
    async (action: string, payload: Record<string, unknown> = {}) => {
      return invoke<unknown>("call_plugin", {
        pluginId: PLUGIN_ID,
        action,
        payload,
      });
    },
    []
  );

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      // Ensure plugin is loaded
      try {
        await invoke("load_plugin", { pluginId: PLUGIN_ID });
      } catch {
        // Already loaded, ignore
      }

      const checkResult = (await callPlugin("check")) as PlatformCheck;
      setCheck(checkResult);

      const mountList = (await callPlugin("list_mounts")) as MountInfo[];
      setMounts(mountList);
    } catch (e) {
      console.error("Failed to load storage plugin:", e);
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [callPlugin]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const handleMount = async () => {
    if (!server.trim() || !share.trim()) return;
    setMounting(true);
    setError("");
    try {
      await callPlugin("mount", {
        server: server.trim(),
        share: share.trim(),
        username: username.trim(),
        password,
        mount_point: mountPoint.trim(),
      });
      // Refresh mounts
      const mountList = (await callPlugin("list_mounts")) as MountInfo[];
      setMounts(mountList);
      // Clear form
      setServer("");
      setShare("");
      setUsername("");
      setPassword("");
      setMountPoint("");
    } catch (e) {
      setError(String(e));
    } finally {
      setMounting(false);
    }
  };

  const handleUnmount = async (mountPoint: string) => {
    const confirmed = await ask(
      `确定要卸载 ${mountPoint} 吗？\n如有工作区使用此路径，卸载后将无法访问。`,
      { title: "卸载网络存储", kind: "warning" }
    );
    if (!confirmed) return;
    try {
      await callPlugin("unmount", { mount_point: mountPoint });
      const mountList = (await callPlugin("list_mounts")) as MountInfo[];
      setMounts(mountList);
    } catch (e) {
      setError("卸载失败: " + String(e));
    }
  };

  const handleCreateWorkspace = async (mount: MountInfo) => {
    try {
      const name = `${mount.server}/${mount.share}`;
      await invoke("create_workspace", {
        name,
        path: mount.mount_point,
      });
      alert(`工作区 "${name}" 已创建，可在工作区切换器中选择。`);
    } catch (e) {
      alert("创建工作区失败: " + String(e));
    }
  };

  if (loading) {
    return (
      <div className="text-sm text-muted-foreground">加载中...</div>
    );
  }

  return (
    <div className="space-y-6">
      <div>
        <h3 className="font-medium text-lg">网络存储</h3>
        <p className="text-sm text-muted-foreground mt-1">
          挂载 SMB/CIFS 网络共享（如 NAS）到本地目录，然后创建工作区进行管理。
        </p>
      </div>

      {check && !check.supported && (
        <div className="rounded-lg border border-yellow-300 bg-yellow-50 dark:bg-yellow-950/20 p-4 flex items-start gap-2">
          <AlertTriangleIcon className="h-4 w-4 text-yellow-600 shrink-0 mt-0.5" />
          <p className="text-sm text-yellow-800 dark:text-yellow-200">
            当前平台 ({check.platform}) 不支持 SMB 挂载。
          </p>
        </div>
      )}

      {/* Mount form */}
      <section className="rounded-lg border p-5 space-y-4">
        <h4 className="font-medium">挂载新的网络共享</h4>

        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-1">
            <Label className="text-sm">服务器地址 *</Label>
            <Input
              value={server}
              onChange={(e) => setServer(e.target.value)}
              placeholder="192.168.1.100"
              className="h-8 text-sm"
            />
          </div>
          <div className="space-y-1">
            <Label className="text-sm">共享名称 *</Label>
            <Input
              value={share}
              onChange={(e) => setShare(e.target.value)}
              placeholder="recordings"
              className="h-8 text-sm"
            />
          </div>
          <div className="space-y-1">
            <Label className="text-sm">用户名</Label>
            <Input
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder="留空使用 guest"
              className="h-8 text-sm"
            />
          </div>
          <div className="space-y-1">
            <Label className="text-sm">密码</Label>
            <Input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="留空无密码"
              className="h-8 text-sm"
            />
          </div>
        </div>

        <div className="space-y-1">
          <Label className="text-sm">本地挂载路径</Label>
          <Input
            value={mountPoint}
            onChange={(e) => setMountPoint(e.target.value)}
            placeholder={
              check?.platform === "windows"
                ? "留空自动分配盘符（或指定如 Z:）"
                : "留空使用默认路径（/tmp/clipper-mounts/...）"
            }
            className="h-8 text-sm font-mono"
          />
        </div>

        {error && (
          <div className="rounded bg-red-50 dark:bg-red-950/20 border border-red-200 p-3 text-sm text-red-700 dark:text-red-300">
            {error}
          </div>
        )}

        <Button
          onClick={handleMount}
          disabled={mounting || !server.trim() || !share.trim()}
          className="gap-1"
        >
          {mounting ? (
            <>
              <Loader2Icon className="h-4 w-4 animate-spin" />
              挂载中...
            </>
          ) : (
            <>
              <PlugIcon className="h-4 w-4" />
              挂载
            </>
          )}
        </Button>
      </section>

      {/* Active mounts */}
      <section className="space-y-3">
        <h4 className="font-medium">已挂载的网络共享</h4>
        {mounts.length === 0 ? (
          <div className="rounded-lg border border-dashed p-6 text-center text-sm text-muted-foreground">
            暂无已挂载的网络共享
          </div>
        ) : (
          <div className="space-y-2">
            {mounts.map((mount) => (
              <div
                key={mount.mount_point}
                className="rounded-lg border p-4 flex items-center justify-between"
              >
                <div className="flex items-center gap-3 min-w-0">
                  <HardDriveIcon className="h-5 w-5 text-primary shrink-0" />
                  <div className="min-w-0">
                    <p className="text-sm font-medium truncate">
                      //{mount.server}/{mount.share}
                    </p>
                    <p className="text-xs text-muted-foreground font-mono truncate">
                      → {mount.mount_point}
                    </p>
                  </div>
                </div>
                <div className="flex items-center gap-2 shrink-0">
                  <Button
                    variant="outline"
                    size="sm"
                    className="gap-1 text-xs"
                    onClick={() => handleCreateWorkspace(mount)}
                  >
                    <FolderPlusIcon className="h-3 w-3" />
                    创建工作区
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="gap-1 text-xs text-destructive hover:text-destructive"
                    onClick={() => handleUnmount(mount.mount_point)}
                  >
                    <UnplugIcon className="h-3 w-3" />
                    卸载
                  </Button>
                </div>
              </div>
            ))}
          </div>
        )}
      </section>

      {/* Tips */}
      <section className="rounded-lg bg-muted/50 p-4 space-y-2">
        <h4 className="font-medium text-sm">使用说明</h4>
        <ul className="text-xs text-muted-foreground space-y-1">
          <li>• 输入 NAS 或网络共享的地址和共享名称，点击挂载</li>
          <li>• 挂载成功后，点击"创建工作区"将该路径添加为工作区</li>
          <li>• 应用退出时会自动卸载所有已挂载的网络共享</li>
          {check?.platform === "linux" && (
            <li className="text-yellow-600">
              • Linux: 可能需要安装 cifs-utils 包并配置 sudo 权限
            </li>
          )}
          {check?.platform === "windows" && (
            <li>
              • Windows: 可指定盘符（如 Z:）或留空自动分配
            </li>
          )}
        </ul>
      </section>
    </div>
  );
}
