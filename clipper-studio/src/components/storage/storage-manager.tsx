import { useState, useEffect, useCallback, useRef } from "react";
import {
  HardDriveIcon,
  PlugIcon,
  UnplugIcon,
  Loader2Icon,
  FolderPlusIcon,
  AlertTriangleIcon,
  HistoryIcon,
  TrashIcon,
  PlayIcon,
  EyeIcon,
  EyeOffIcon,
  FolderOpenIcon,
} from "lucide-react";
import { ask, open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { useNavigate } from "@tanstack/react-router";
import { useWorkspaceStore } from "@/stores/workspace";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { getPluginConfig, setPluginConfig } from "@/services/plugin";

const PLUGIN_ID = "builtin.storage.smb";
const HISTORY_KEY = "mount_history";

interface MountInfo {
  server: string;
  share: string;
  mount_point: string;
}

interface PlatformCheck {
  supported: boolean;
  platform: string;
}

interface MountHistoryEntry {
  server: string;
  share: string;
  username: string;
  password: string;
  mountPoint: string;
  lastUsed: string; // ISO date string
}

export function StorageManager() {
  const navigate = useNavigate();
  const recheckPath = useWorkspaceStore((s) => s.recheckPath);
  const [mounts, setMounts] = useState<MountInfo[]>([]);
  const [check, setCheck] = useState<PlatformCheck | null>(null);
  const [loading, setLoading] = useState(true);
  const [mounting, setMounting] = useState(false);
  const [error, setError] = useState("");
  const [history, setHistory] = useState<MountHistoryEntry[]>([]);
  const historyRef = useRef<MountHistoryEntry[]>([]);

  // Form state
  const [server, setServer] = useState("");
  const [share, setShare] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
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

  // Save mount history to plugin config
  const saveHistory = useCallback(async (entries: MountHistoryEntry[]) => {
    historyRef.current = entries;
    setHistory(entries);
    await setPluginConfig(PLUGIN_ID, HISTORY_KEY, JSON.stringify(entries));
  }, []);

  // Add or update a history entry (by server+share key)
  const upsertHistory = useCallback(
    async (entry: Omit<MountHistoryEntry, "lastUsed">) => {
      const now = new Date().toISOString();
      const key = `${entry.server}/${entry.share}`;
      const updated = historyRef.current.filter(
        (h) => `${h.server}/${h.share}` !== key
      );
      updated.unshift({ ...entry, lastUsed: now });
      // Keep at most 20 history entries
      await saveHistory(updated.slice(0, 20));
    },
    [saveHistory]
  );

  const deleteHistory = useCallback(
    async (entry: MountHistoryEntry) => {
      const key = `${entry.server}/${entry.share}`;
      const updated = historyRef.current.filter(
        (h) => `${h.server}/${h.share}` !== key
      );
      await saveHistory(updated);
    },
    [saveHistory]
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

      // Load history first, then detect which are still mounted at OS level
      let historyEntries: MountHistoryEntry[] = [];
      try {
        const cfg = await getPluginConfig(PLUGIN_ID);
        const raw = cfg[HISTORY_KEY];
        if (raw) {
          historyEntries = JSON.parse(raw) as MountHistoryEntry[];
          historyRef.current = historyEntries;
          setHistory(historyEntries);
        }
      } catch {
        // Ignore parse errors
      }

      // Build candidates from history for OS-level detection
      if (historyEntries.length > 0) {
        const candidates = historyEntries
          .filter((h) => h.mountPoint)
          .map((h) => ({
            server: h.server,
            share: h.share,
            mount_point: h.mountPoint,
          }));
        if (candidates.length > 0) {
          await callPlugin("detect_mounts", { candidates });
        }
      }

      // Now list_mounts returns both in-memory + detected mounts
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

  const doMount = async (
    s: string,
    sh: string,
    u: string,
    p: string,
    mp: string
  ) => {
    await callPlugin("mount", {
      server: s,
      share: sh,
      username: u,
      password: p,
      mount_point: mp,
    });
    // Save to history
    await upsertHistory({
      server: s,
      share: sh,
      username: u,
      password: p,
      mountPoint: mp,
    });
    // Refresh mounts & workspace path status
    const mountList = (await callPlugin("list_mounts")) as MountInfo[];
    setMounts(mountList);
    recheckPath();
  };

  const handleMount = async () => {
    if (!server.trim() || !share.trim()) return;
    setMounting(true);
    setError("");
    try {
      // "auto" means let the backend auto-assign
      const mp = mountPoint === "auto" ? "" : mountPoint.trim();
      await doMount(
        server.trim(),
        share.trim(),
        username.trim(),
        password,
        mp
      );
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

  const handleRemount = async (entry: MountHistoryEntry) => {
    setMounting(true);
    setError("");
    try {
      await doMount(
        entry.server,
        entry.share,
        entry.username,
        entry.password,
        entry.mountPoint
      );
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
      // Recheck workspace path after unmount
      recheckPath();
    } catch (e) {
      setError("卸载失败: " + String(e));
    }
  };

  const handleCreateWorkspace = (mount: MountInfo) => {
    const adapterConfig = JSON.stringify({
      source: "smb",
      server: mount.server,
      share: mount.share,
      mount_point: mount.mount_point,
    });
    navigate({
      to: "/welcome",
      search: {
        name: `${mount.server}/${mount.share}`,
        path: mount.mount_point,
        step: "import",
        adapter_config: adapterConfig,
      },
    });
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
            <Label className="text-sm">共享文件夹路径 *</Label>
            <Input
              value={share}
              onChange={(e) => setShare(e.target.value)}
              placeholder="share/subfolder/path"
              className="h-8 text-sm"
            />
            <p className="text-[11px] text-muted-foreground">
              SMB 共享名称或子路径，如 recordings 或 nas/videos
            </p>
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
            <div className="relative">
              <Input
                type={showPassword ? "text" : "password"}
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="留空无密码"
                className="h-8 text-sm pr-8"
              />
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="absolute right-0 top-0 h-8 w-8 px-0"
                onClick={() => setShowPassword((v) => !v)}
                tabIndex={-1}
              >
                {showPassword ? (
                  <EyeOffIcon className="h-3.5 w-3.5 text-muted-foreground" />
                ) : (
                  <EyeIcon className="h-3.5 w-3.5 text-muted-foreground" />
                )}
              </Button>
            </div>
          </div>
        </div>

        <div className="space-y-1">
          <Label className="text-sm">
            {check?.platform === "windows" ? "映射盘符" : "本地挂载路径"}
          </Label>
          {check?.platform === "windows" ? (
            <Select value={mountPoint} onValueChange={setMountPoint}>
              <SelectTrigger className="h-8 text-sm font-mono w-40">
                <SelectValue placeholder="自动分配" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="auto">自动分配</SelectItem>
                {Array.from({ length: 26 }, (_, i) =>
                  String.fromCharCode(65 + i)
                ).map((letter) => (
                  <SelectItem key={letter} value={`${letter}:`}>
                    {letter}:
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          ) : (
            <div className="flex gap-2">
              <Input
                value={mountPoint}
                onChange={(e) => setMountPoint(e.target.value)}
                placeholder="留空使用默认路径（/tmp/clipper-mounts/...）"
                className="h-8 text-sm font-mono flex-1"
              />
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="h-8 px-2 shrink-0"
                onClick={async () => {
                  const selected = await open({ directory: true, multiple: false });
                  if (selected) setMountPoint(selected as string);
                }}
              >
                <FolderOpenIcon className="h-4 w-4" />
              </Button>
            </div>
          )}
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

      {/* Mount history */}
      {history.length > 0 && (
        <section className="space-y-3">
          <h4 className="font-medium flex items-center gap-1.5">
            <HistoryIcon className="h-4 w-4" />
            挂载历史
          </h4>
          <div className="space-y-2">
            {history.map((entry) => {
              const key = `${entry.server}/${entry.share}`;
              // Check if already mounted
              const isActive = mounts.some(
                (m) => m.server === entry.server && m.share === entry.share
              );
              return (
                <div
                  key={key}
                  className="rounded-lg border p-3 flex items-center justify-between"
                >
                  <div className="min-w-0">
                    <p className="text-sm font-medium truncate">
                      //{entry.server}/{entry.share}
                    </p>
                    <div className="flex gap-3 text-xs text-muted-foreground mt-0.5">
                      {entry.username && <span>用户: {entry.username}</span>}
                      {entry.mountPoint && (
                        <span className="font-mono">{entry.mountPoint}</span>
                      )}
                      <span>
                        {new Date(entry.lastUsed).toLocaleDateString()}
                      </span>
                    </div>
                  </div>
                  <div className="flex items-center gap-2 shrink-0">
                    {isActive ? (
                      <span className="text-xs text-green-600 px-2 py-0.5 rounded bg-green-50 dark:bg-green-950/20">
                        已挂载
                      </span>
                    ) : (
                      <Button
                        variant="outline"
                        size="sm"
                        className="gap-1 text-xs"
                        disabled={mounting}
                        onClick={() => handleRemount(entry)}
                      >
                        <PlayIcon className="h-3 w-3" />
                        重新挂载
                      </Button>
                    )}
                    <Button
                      variant="ghost"
                      size="sm"
                      className="gap-1 text-xs text-muted-foreground hover:text-destructive"
                      onClick={() => deleteHistory(entry)}
                    >
                      <TrashIcon className="h-3 w-3" />
                    </Button>
                  </div>
                </div>
              );
            })}
          </div>
        </section>
      )}

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
              • Windows: 选择要映射的盘符，或选择"自动分配"由系统决定
            </li>
          )}
        </ul>
      </section>
    </div>
  );
}
