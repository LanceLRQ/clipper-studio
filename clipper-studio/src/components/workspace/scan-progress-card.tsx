import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import { XIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { cancelScan } from "@/services/workspace";
import type { ScanProgressPayload, ScanResult, ScanStage } from "@/services/workspace";
import type { TaskProgressEvent } from "@/types/clip";

interface ScanProgressCardProps {
  taskId: number;
  onComplete: (result: ScanResult | null) => void;
  onCancelled: () => void;
  onFailed: (message: string) => void;
}

const STAGE_META: Record<ScanStage, { icon: string; title: string }> = {
  preparing: { icon: "🧹", title: "清理旧记录…" },
  scanning: { icon: "📁", title: "扫描目录结构…" },
  probing: { icon: "🔍", title: "解析视频元数据" },
  grouping: { icon: "🗂️", title: "分组为场次…" },
  writing: { icon: "💾", title: "写入数据库" },
};

function formatSec(s: number): string {
  const sec = Math.max(0, Math.floor(s));
  const m = Math.floor(sec / 60);
  const r = sec % 60;
  return `${String(m).padStart(2, "0")}:${String(r).padStart(2, "0")}`;
}

function parsePayload(message: string): ScanProgressPayload | null {
  try {
    const obj = JSON.parse(message);
    if (obj && typeof obj === "object" && "stage" in obj) {
      return obj as ScanProgressPayload;
    }
  } catch {
    // 非 JSON 消息（来自其他任务），忽略
  }
  return null;
}

export function ScanProgressCard({
  taskId,
  onComplete,
  onCancelled,
  onFailed,
}: ScanProgressCardProps) {
  const [progress, setProgress] = useState(0);
  const [payload, setPayload] = useState<ScanProgressPayload | null>(null);
  const [status, setStatus] = useState<"running" | "failed">("running");
  const [errorMsg, setErrorMsg] = useState<string>("");
  const [elapsed, setElapsed] = useState(0);
  const [cancelling, setCancelling] = useState(false);
  const startedAtRef = useRef<number>(Date.now());
  const resultRef = useRef<ScanResult | null>(null);

  // 监听 task-progress 事件
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    listen<TaskProgressEvent>("task-progress", (event) => {
      if (event.payload.task_id !== taskId) return;
      const p = event.payload;
      setProgress(p.progress);
      const parsed = parsePayload(p.message);
      if (parsed) {
        setPayload(parsed);
        if (parsed.result) resultRef.current = parsed.result;
      }
      if (p.status === "completed") {
        onComplete(resultRef.current);
      } else if (p.status === "cancelled") {
        onCancelled();
      } else if (p.status === "failed") {
        setStatus("failed");
        setErrorMsg(p.message || "扫描失败");
        onFailed(p.message || "扫描失败");
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [taskId, onComplete, onCancelled, onFailed]);

  // 每秒刷新已用时
  useEffect(() => {
    const timer = setInterval(() => {
      setElapsed((Date.now() - startedAtRef.current) / 1000);
    }, 1000);
    return () => clearInterval(timer);
  }, []);

  const handleCancel = async () => {
    if (cancelling) return;
    const ok = await ask("确定取消扫描？", {
      title: "取消扫描",
      kind: "warning",
    });
    if (!ok) return;
    setCancelling(true);
    await cancelScan(taskId);
  };

  const stage = payload?.stage ?? "preparing";
  const meta = STAGE_META[stage];
  const pct = Math.round(progress * 100);
  const current = payload?.current;
  const total = payload?.total;

  // 预计剩余（仅在 probing 阶段且已有进度时计算）
  const remaining =
    stage === "probing" && progress > 0.15 && progress < 0.85
      ? elapsed * (1 - progress) / progress
      : null;

  // 子行内容
  let subtitle = "";
  let subPath = "";
  if (stage === "scanning" && total !== undefined) {
    subtitle = `发现 ${total} 个视频文件`;
  } else if (stage === "probing" && payload?.file) {
    subtitle = `正在处理：${payload.file}`;
    subPath = payload.path ?? "";
  } else if (stage === "grouping" && total !== undefined) {
    subtitle = `已识别 ${total} 个主播`;
  } else if (stage === "writing" && current !== undefined && total !== undefined) {
    subtitle = `${current}/${total} 场次写入完成`;
  }

  const isFailed = status === "failed";
  const borderColor = isFailed
    ? "border-destructive"
    : cancelling
      ? "border-muted"
      : "border-primary/40";

  return (
    <div className={`rounded-lg border ${borderColor} bg-card p-4 shadow-sm`}>
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2 text-sm font-medium">
          <span className="text-lg">{isFailed ? "❌" : meta.icon}</span>
          <span>{isFailed ? "扫描失败" : meta.title}</span>
        </div>
        <div className="text-xs text-muted-foreground tabular-nums">
          {current !== undefined && total !== undefined
            ? `${current} / ${total}  `
            : ""}
          ({pct}%)
        </div>
      </div>

      <Progress
        value={pct}
        className="mt-3"
        indicatorClassName={
          isFailed ? "bg-destructive" : cancelling ? "bg-muted-foreground" : undefined
        }
      />

      {subtitle && !isFailed && (
        <div className="mt-3 space-y-0.5">
          <div className="truncate text-sm">{subtitle}</div>
          {subPath && (
            <div className="truncate text-xs text-muted-foreground">{subPath}</div>
          )}
        </div>
      )}

      {isFailed && (
        <div className="mt-3 text-sm text-destructive">{errorMsg}</div>
      )}

      <div className="mt-3 flex items-center justify-between">
        <div className="text-xs text-muted-foreground tabular-nums">
          已用时 {formatSec(elapsed)}
          {remaining !== null && ` · 预计剩余 ${formatSec(remaining)}`}
        </div>
        {!isFailed && (
          <Button
            variant="outline"
            size="sm"
            onClick={handleCancel}
            disabled={cancelling}
          >
            <XIcon className="mr-1 h-3 w-3" />
            {cancelling ? "取消中…" : "取消扫描"}
          </Button>
        )}
      </div>
    </div>
  );
}
