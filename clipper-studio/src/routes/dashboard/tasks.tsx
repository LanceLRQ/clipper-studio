import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Button } from "@/components/ui/button";
import type { ClipTaskInfo, TaskProgressEvent } from "@/types/clip";
import { listClipTasks, cancelClip } from "@/services/clip";

function formatDuration(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const m = Math.floor(totalSec / 60);
  const s = totalSec % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

const statusLabels: Record<string, { text: string; color: string }> = {
  pending: { text: "等待中", color: "text-yellow-600" },
  processing: { text: "处理中", color: "text-blue-600" },
  completed: { text: "已完成", color: "text-green-600" },
  failed: { text: "失败", color: "text-red-500" },
  cancelled: { text: "已取消", color: "text-muted-foreground" },
};

function TasksPage() {
  const [tasks, setTasks] = useState<ClipTaskInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [liveProgress, setLiveProgress] = useState<
    Record<number, TaskProgressEvent>
  >({});

  const loadTasks = async () => {
    try {
      const t = await listClipTasks();
      setTasks(t);
    } catch (e) {
      console.error("Failed to load tasks:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadTasks();
  }, []);

  // Listen for real-time task progress
  useEffect(() => {
    const unlisten = listen<TaskProgressEvent>("task-progress", (event) => {
      const p = event.payload;
      setLiveProgress((prev) => ({ ...prev, [p.task_id]: p }));

      // Refresh task list when a task completes/fails
      if (["completed", "failed", "cancelled"].includes(p.status)) {
        setTimeout(loadTasks, 500);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const handleCancel = async (taskId: number) => {
    await cancelClip(taskId);
    await loadTasks();
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-2xl font-semibold">任务中心</h2>
        <Button variant="outline" size="sm" onClick={loadTasks}>
          刷新
        </Button>
      </div>

      {loading ? (
        <div className="text-muted-foreground">加载中...</div>
      ) : tasks.length === 0 ? (
        <div className="rounded-lg border border-dashed p-12 text-center text-muted-foreground">
          暂无任务，在视频详情页中创建切片任务
        </div>
      ) : (
        <div className="space-y-2">
          {tasks.map((task) => {
            const live = liveProgress[task.id];
            const status = live?.status ?? task.status;
            const progress = live?.progress ?? task.progress;
            const label = statusLabels[status] ?? statusLabels.pending;

            return (
              <div
                key={task.id}
                className="rounded-lg border p-4 space-y-2"
              >
                <div className="flex items-center justify-between">
                  <div>
                    <span className="font-medium">
                      {task.title ?? `切片 #${task.id}`}
                    </span>
                    <span className="ml-2 text-xs text-muted-foreground">
                      {formatDuration(task.start_time_ms)} →{" "}
                      {formatDuration(task.end_time_ms)}
                    </span>
                  </div>
                  <div className="flex items-center gap-2">
                    <span className={`text-sm ${label.color}`}>
                      {label.text}
                    </span>
                    {status === "processing" && (
                      <Button
                        variant="ghost"
                        size="sm"
                        className="text-red-500"
                        onClick={() => handleCancel(task.id)}
                      >
                        取消
                      </Button>
                    )}
                  </div>
                </div>

                {/* Progress bar for processing tasks */}
                {status === "processing" && (
                  <div className="space-y-1">
                    <div className="w-full h-2 bg-muted rounded-full overflow-hidden">
                      <div
                        className="h-full bg-primary transition-all duration-300"
                        style={{ width: `${progress * 100}%` }}
                      />
                    </div>
                    <div className="text-xs text-muted-foreground">
                      {live?.message ?? `${Math.round(progress * 100)}%`}
                    </div>
                  </div>
                )}

                {/* Error message */}
                {status === "failed" && task.error_message && (
                  <div className="text-xs text-red-500">
                    {task.error_message}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

export const Route = createFileRoute("/dashboard/tasks")({
  component: TasksPage,
});
