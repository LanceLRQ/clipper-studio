import { useState, useEffect, useMemo } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ArrowLeft } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  createWorkspace,
  scanWorkspace,
  detectWorkspaceAdapter,
  listWorkspaces,
} from "@/services/workspace";
import type { WorkspaceInfo } from "@/types/workspace";
import { useWorkspaceStore } from "@/stores/workspace";

/**
 * 把绝对路径归一化：去掉末尾分隔符、统一为正斜杠。
 * 这样可以简单通过 startsWith 判断父子关系。
 */
function normalizePath(p: string): string {
  return p.replace(/\\/g, "/").replace(/\/+$/, "");
}

type PathOverlap =
  | { kind: "same"; other: WorkspaceInfo }
  | { kind: "child"; other: WorkspaceInfo }
  | { kind: "parent"; other: WorkspaceInfo };

function detectPathOverlap(
  picked: string,
  workspaces: WorkspaceInfo[]
): PathOverlap | null {
  const a = normalizePath(picked);
  if (!a) return null;
  for (const ws of workspaces) {
    const b = normalizePath(ws.path);
    if (!b) continue;
    if (a === b) return { kind: "same", other: ws };
    if (a.startsWith(b + "/")) return { kind: "child", other: ws };
    if (b.startsWith(a + "/")) return { kind: "parent", other: ws };
  }
  return null;
}

export type WorkspaceStepMode = "choose" | "import" | "create";

type AdapterChoice = "auto" | "bililive-recorder" | "generic";

const ADAPTER_OPTIONS: { value: AdapterChoice; label: string; hint: string }[] = [
  { value: "auto", label: "自动检测", hint: "根据目录结构自动识别" },
  {
    value: "bililive-recorder",
    label: "录播姬 (BililiveRecorder)",
    hint: "解析 flv.xml、info.json 等元信息",
  },
  { value: "generic", label: "通用目录", hint: "仅按文件名递归导入视频" },
];

interface WorkspaceStepProps {
  mode: WorkspaceStepMode;
  onModeChange: (mode: WorkspaceStepMode) => void;
  initialName?: string;
  initialPath?: string;
  adapterConfig?: string;
  onHasExistingChange?: (hasExisting: boolean) => void;
  onCreated: (workspaceId: string) => void;
}

export function WorkspaceStep({
  mode,
  onModeChange,
  initialName,
  initialPath,
  adapterConfig,
  onHasExistingChange,
  onCreated,
}: WorkspaceStepProps) {
  const switchWorkspace = useWorkspaceStore((s) => s.switchWorkspace);
  const [name, setName] = useState(initialName || "");
  const [path, setPath] = useState(initialPath || "");
  const [adapterChoice, setAdapterChoice] = useState<AdapterChoice>("auto");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [existingWorkspaces, setExistingWorkspaces] = useState<WorkspaceInfo[]>(
    []
  );

  useEffect(() => {
    listWorkspaces()
      .then((ws) => {
        setExistingWorkspaces(ws);
        onHasExistingChange?.(ws.length > 0);
      })
      .catch(() => {});
  }, [onHasExistingChange]);

  const overlap = useMemo(
    () => (path.trim() ? detectPathOverlap(path.trim(), existingWorkspaces) : null),
    [path, existingWorkspaces]
  );

  const handlePickFolder = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (selected) {
      setPath(selected as string);
      const folderName = (selected as string).split(/[/\\]/).pop() ?? "";
      if (!name) setName(folderName);
    }
  };

  const handleCreate = async () => {
    if (!name.trim()) {
      setError("请输入工作区名称");
      return;
    }
    if (!path.trim()) {
      setError("请选择文件夹");
      return;
    }

    if (overlap) {
      const reason =
        overlap.kind === "same"
          ? `与工作区「${overlap.other.name}」指向同一目录`
          : overlap.kind === "child"
            ? `位于工作区「${overlap.other.name}」内部`
            : `包含工作区「${overlap.other.name}」`;
      setError(`${reason}，请换一个位置。`);
      return;
    }

    setLoading(true);
    setError("");
    try {
      const adapterId =
        adapterChoice === "auto"
          ? await detectWorkspaceAdapter(path.trim())
          : adapterChoice;
      const ws = await createWorkspace({
        name: name.trim(),
        path: path.trim(),
        adapter_id: adapterId,
        adapter_config: adapterConfig || undefined,
      });
      await switchWorkspace(ws.id);
      if (mode === "import") {
        try {
          await scanWorkspace(ws.id);
        } catch (scanErr) {
          console.warn("Scan failed (non-fatal):", scanErr);
        }
      }
      onCreated(ws.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  if (mode === "choose") {
    return (
      <div className="space-y-4">
        <p className="text-center text-sm text-muted-foreground">
          选择一种方式开始：
        </p>
        <div className="grid gap-4">
          <button
            onClick={() => onModeChange("import")}
            className="rounded-lg border-2 border-dashed p-6 text-left hover:border-primary hover:bg-accent transition-colors"
          >
            <div className="text-lg font-medium">导入已有录播目录</div>
            <div className="text-sm text-muted-foreground mt-1">
              选择 BililiveRecorder 等录制工具的工作目录，自动识别并导入视频
            </div>
          </button>
          <button
            onClick={() => onModeChange("create")}
            className="rounded-lg border-2 border-dashed p-6 text-left hover:border-primary hover:bg-accent transition-colors"
          >
            <div className="text-lg font-medium">创建全新工作区</div>
            <div className="text-sm text-muted-foreground mt-1">
              选择一个空文件夹作为新的工作区，手动导入视频文件
            </div>
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-md space-y-6">
      <div className="flex items-center gap-2">
        <Button
          variant="ghost"
          size="sm"
          onClick={() => {
            onModeChange("choose");
            setError("");
          }}
        >
          <ArrowLeft className="mr-1 h-4 w-4" />
          返回
        </Button>
        <h2 className="text-lg font-medium">
          {mode === "import" ? "导入已有录播目录" : "创建全新工作区"}
        </h2>
      </div>

      <div className="space-y-4">
        <div className="space-y-2">
          <Label htmlFor="path">
            {mode === "import" ? "选择录播目录" : "选择文件夹"}
          </Label>
          <div className="flex gap-2">
            <Input
              id="path"
              value={path}
              onChange={(e) => setPath(e.target.value)}
              placeholder={
                mode === "import"
                  ? "BililiveRecorder 工作目录路径"
                  : "新工作区的文件夹路径"
              }
              readOnly
            />
            <Button variant="outline" onClick={handlePickFolder}>
              浏览
            </Button>
          </div>
          {overlap && (
            <p className="text-xs text-amber-600">
              {overlap.kind === "same" &&
                `与工作区「${overlap.other.name}」指向同一目录，无法创建。`}
              {overlap.kind === "child" &&
                `位于工作区「${overlap.other.name}」内部，扫描时重叠文件会被跳过。`}
              {overlap.kind === "parent" &&
                `包含工作区「${overlap.other.name}」，扫描时重叠文件会被跳过。`}
            </p>
          )}
        </div>

        <div className="space-y-2">
          <Label htmlFor="name">工作区名称</Label>
          <Input
            id="name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="例如：Bilibili录播、抖音录播..."
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor="adapter">适配器类型</Label>
          <select
            id="adapter"
            value={adapterChoice}
            onChange={(e) => setAdapterChoice(e.target.value as AdapterChoice)}
            className="h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm"
          >
            {ADAPTER_OPTIONS.map((opt) => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </select>
          <p className="text-xs text-muted-foreground">
            {ADAPTER_OPTIONS.find((o) => o.value === adapterChoice)?.hint}
          </p>
        </div>

        {error && <div className="text-sm text-red-500">{error}</div>}

        <Button
          size="lg"
          className="mt-6 w-full"
          onClick={handleCreate}
          disabled={loading || !path || !name || !!overlap}
        >
          {loading ? "创建中..." : "创建并继续"}
        </Button>
      </div>
    </div>
  );
}
