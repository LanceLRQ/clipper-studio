import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  createWorkspace,
  scanWorkspace,
  detectWorkspaceAdapter,
} from "@/services/workspace";
import { useWorkspaceStore } from "@/stores/workspace";

type WizardStep = "choose" | "import" | "create";

function WelcomePage() {
  const navigate = useNavigate();
  const switchWorkspace = useWorkspaceStore((s) => s.switchWorkspace);
  const [step, setStep] = useState<WizardStep>("choose");
  const [name, setName] = useState("");
  const [path, setPath] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const handlePickFolder = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (selected) {
      setPath(selected as string);
      // Auto-generate name from folder name
      const folderName = (selected as string).split(/[/\\]/).pop() ?? "";
      if (!name) {
        setName(folderName);
      }
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

    setLoading(true);
    setError("");
    try {
      // Auto-detect adapter type
      const adapterId = await detectWorkspaceAdapter(path.trim());
      const ws = await createWorkspace({
        name: name.trim(),
        path: path.trim(),
        adapter_id: adapterId,
      });

      // Activate the newly created workspace
      await switchWorkspace(ws.id);

      // Auto-scan after creation (for "import" mode)
      if (step === "import") {
        try {
          const result = await scanWorkspace(ws.id);
          console.log("Scan result:", result);
        } catch (scanErr) {
          console.warn("Scan failed (non-fatal):", scanErr);
        }
      }

      navigate({ to: "/dashboard/videos" });
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex h-screen items-center justify-center">
      <div className="mx-auto max-w-lg space-y-8 p-8">
        <div className="text-center space-y-2">
          <h1 className="text-3xl font-bold">欢迎使用 ClipperStudio</h1>
          <p className="text-muted-foreground">
            本地优先的桌面视频工作台，面向直播录播切片创作者
          </p>
        </div>

        {step === "choose" && (
          <div className="space-y-4">
            <p className="text-center text-sm text-muted-foreground">
              选择一种方式开始：
            </p>
            <div className="grid gap-4">
              <button
                onClick={() => setStep("import")}
                className="rounded-lg border-2 border-dashed p-6 text-left hover:border-primary hover:bg-accent transition-colors"
              >
                <div className="text-lg font-medium">导入已有录播目录</div>
                <div className="text-sm text-muted-foreground mt-1">
                  选择 BililiveRecorder 等录制工具的工作目录，自动识别并导入视频
                </div>
              </button>
              <button
                onClick={() => setStep("create")}
                className="rounded-lg border-2 border-dashed p-6 text-left hover:border-primary hover:bg-accent transition-colors"
              >
                <div className="text-lg font-medium">创建全新工作区</div>
                <div className="text-sm text-muted-foreground mt-1">
                  选择一个空文件夹作为新的工作区，手动导入视频文件
                </div>
              </button>
            </div>
          </div>
        )}

        {(step === "import" || step === "create") && (
          <div className="space-y-6">
            <div className="flex items-center gap-2">
              <Button variant="ghost" size="sm" onClick={() => { setStep("choose"); setError(""); }}>
                ← 返回
              </Button>
              <h2 className="text-lg font-medium">
                {step === "import" ? "导入已有录播目录" : "创建全新工作区"}
              </h2>
            </div>

            <div className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="path">
                  {step === "import" ? "选择录播目录" : "选择文件夹"}
                </Label>
                <div className="flex gap-2">
                  <Input
                    id="path"
                    value={path}
                    onChange={(e) => setPath(e.target.value)}
                    placeholder={
                      step === "import"
                        ? "BililiveRecorder 工作目录路径"
                        : "新工作区的文件夹路径"
                    }
                    readOnly
                  />
                  <Button variant="outline" onClick={handlePickFolder}>
                    浏览
                  </Button>
                </div>
              </div>

              <div className="space-y-2">
                <Label htmlFor="name">工作区名称</Label>
                <Input
                  id="name"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="例如：说说Crystal录播"
                />
              </div>

              {error && (
                <div className="text-sm text-red-500">{error}</div>
              )}

              <Button
                className="w-full"
                onClick={handleCreate}
                disabled={loading || !path || !name}
              >
                {loading ? "创建中..." : "开始使用"}
              </Button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

export const Route = createFileRoute("/welcome")({
  component: WelcomePage,
});
