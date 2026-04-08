import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useMemo, useState, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
  DialogClose,
} from "@/components/ui/dialog";
import type { ClipTaskInfo, TaskProgressEvent } from "@/types/clip";
import {
  listClipTasks,
  cancelClip,
  deleteClipTask,
  deleteClipBatch,
  clearFinishedClipTasks,
} from "@/services/clip";
import { clearFinishedMediaTasks } from "@/services/media";

function formatTime(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const h = Math.floor(totalSec / 3600);
  const m = Math.floor((totalSec % 3600) / 60);
  const s = totalSec % 60;
  if (h > 0)
    return `${h}:${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
  return `${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
}

function formatDurationMs(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const m = Math.floor(totalSec / 60);
  const s = totalSec % 60;
  return `${m}分${s}秒`;
}

const statusLabels: Record<string, { text: string; color: string }> = {
  pending: { text: "等待中", color: "text-yellow-600" },
  processing: { text: "处理中", color: "text-blue-600" },
  completed: { text: "已完成", color: "text-green-600" },
  failed: { text: "失败", color: "text-red-500" },
  cancelled: { text: "已取消", color: "text-muted-foreground" },
};

/** A group of tasks — either a batch or a single standalone task */
interface TaskGroup {
  key: string;
  title: string;
  tasks: ClipTaskInfo[];
  isBatch: boolean;
}

function TasksPage() {
  const [tasks, setTasks] = useState<ClipTaskInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [liveProgress, setLiveProgress] = useState<
    Record<number, TaskProgressEvent>
  >({});
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(
    new Set(),
  );

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

  // Delete confirmation dialog state
  const [deleteDialog, setDeleteDialog] = useState<{
    open: boolean;
    title: string;
    description: string;
    deleteFiles: boolean;
    onConfirm: (deleteFiles: boolean) => Promise<void>;
  }>({
    open: false,
    title: "",
    description: "",
    deleteFiles: false,
    onConfirm: async () => {},
  });

  const showDeleteDialog = useCallback(
    (
      title: string,
      description: string,
      onConfirm: (deleteFiles: boolean) => Promise<void>,
    ) => {
      setDeleteDialog({
        open: true,
        title,
        description,
        deleteFiles: false,
        onConfirm,
      });
    },
    [],
  );

  const handleDialogConfirm = async () => {
    const { deleteFiles, onConfirm } = deleteDialog;
    setDeleteDialog((prev) => ({ ...prev, open: false }));
    try {
      await onConfirm(deleteFiles);
    } catch (e) {
      alert(String(e));
      return;
    }
    await loadTasks();
  };

  const handleCancel = async (taskId: number) => {
    await cancelClip(taskId);
    await loadTasks();
  };

  const handleDeleteTask = (taskId: number) => {
    showDeleteDialog("删除任务", "确定要删除该任务记录吗？", async (df) => {
      await deleteClipTask(taskId, df);
    });
  };

  const handleDeleteBatch = (batchId: string, title: string) => {
    showDeleteDialog(
      "删除批次",
      `确定要删除批次「${title}」的任务记录吗？\n处理中或等待中的任务不会被删除。`,
      async (df) => {
        await deleteClipBatch(batchId, df);
      },
    );
  };

  const handleClearAll = () => {
    showDeleteDialog(
      "清除任务",
      "确定要清除所有已完成、失败和已取消的任务记录吗？",
      async (df) => {
        await clearFinishedClipTasks(df);
        await clearFinishedMediaTasks(df);
      },
    );
  };

  const hasFinishedTasks = tasks.some((t) =>
    ["completed", "failed", "cancelled"].includes(t.status),
  );

  // Group tasks by batch_id
  const groups: TaskGroup[] = useMemo(() => {
    const batchMap = new Map<string, ClipTaskInfo[]>();
    const standalone: ClipTaskInfo[] = [];

    for (const task of tasks) {
      if (task.batch_id) {
        if (!batchMap.has(task.batch_id)) {
          batchMap.set(task.batch_id, []);
        }
        batchMap.get(task.batch_id)!.push(task);
      } else {
        standalone.push(task);
      }
    }

    const result: TaskGroup[] = [];

    // Batches (sorted by first task's created_at DESC)
    for (const [batchId, batchTasks] of batchMap) {
      result.push({
        key: batchId,
        title: batchTasks[0]?.batch_title ?? `批次 ${batchId}`,
        tasks: batchTasks,
        isBatch: true,
      });
    }

    // Standalone tasks
    for (const task of standalone) {
      result.push({
        key: `single-${task.id}`,
        title: task.title ?? `切片 #${task.id}`,
        tasks: [task],
        isBatch: false,
      });
    }

    return result;
  }, [tasks]);

  const toggleGroup = (key: string) => {
    setCollapsedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  /** Compute aggregate status for a batch */
  const getBatchStatus = (batchTasks: ClipTaskInfo[]): string => {
    const statuses = batchTasks.map(
      (t) => liveProgress[t.id]?.status ?? t.status,
    );
    if (statuses.some((s) => s === "processing")) return "processing";
    if (statuses.some((s) => s === "pending")) return "pending";
    if (statuses.some((s) => s === "failed")) return "failed";
    if (statuses.every((s) => s === "completed")) return "completed";
    if (statuses.every((s) => s === "cancelled")) return "cancelled";
    return "completed";
  };

  /** Compute aggregate progress for a batch */
  const getBatchProgress = (batchTasks: ClipTaskInfo[]): number => {
    if (batchTasks.length === 0) return 0;
    const total = batchTasks.reduce(
      (sum, t) => sum + (liveProgress[t.id]?.progress ?? t.progress),
      0,
    );
    return total / batchTasks.length;
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-2xl font-semibold">任务中心</h2>
        <div className="flex items-center gap-2">
          {hasFinishedTasks && (
            <Button variant="outline" size="sm" onClick={handleClearAll}>
              清除已完成
            </Button>
          )}
          <Button variant="outline" size="sm" onClick={loadTasks}>
            刷新
          </Button>
        </div>
      </div>

      {loading ? (
        <div className="text-muted-foreground">加载中...</div>
      ) : groups.length === 0 ? (
        <div className="rounded-lg border border-dashed p-12 text-center text-muted-foreground">
          暂无任务，在视频详情页中创建切片任务
        </div>
      ) : (
        <div className="space-y-2">
          {groups.map((group) => {
            if (!group.isBatch) {
              // Single standalone task — render directly
              const task = group.tasks[0];
              const live = liveProgress[task.id];
              const status = live?.status ?? task.status;
              const progress = live?.progress ?? task.progress;
              const label = statusLabels[status] ?? statusLabels.pending;

              return (
                <div
                  key={group.key}
                  className="rounded-lg border p-4 space-y-2"
                >
                  <div className="flex items-center justify-between">
                    <div>
                      <span className="font-medium">{group.title}</span>
                      <span className="ml-2 text-xs text-muted-foreground">
                        {formatTime(task.start_time_ms)} →{" "}
                        {formatTime(task.end_time_ms)}
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
                      {["completed", "failed", "cancelled"].includes(
                        status,
                      ) && (
                        <Button
                          variant="ghost"
                          size="sm"
                          className="text-muted-foreground hover:text-red-500"
                          onClick={() => handleDeleteTask(task.id)}
                        >
                          删除
                        </Button>
                      )}
                    </div>
                  </div>
                  {status === "processing" && (
                    <ProgressBar
                      progress={progress}
                      message={live?.message}
                    />
                  )}
                  {status === "failed" && task.error_message && (
                    <div className="text-xs text-red-500">
                      {task.error_message}
                    </div>
                  )}
                </div>
              );
            }

            // Batch group
            const batchStatus = getBatchStatus(group.tasks);
            const batchProgress = getBatchProgress(group.tasks);
            const batchLabel =
              statusLabels[batchStatus] ?? statusLabels.pending;
            const isCollapsed = collapsedGroups.has(group.key);
            const completedCount = group.tasks.filter(
              (t) =>
                (liveProgress[t.id]?.status ?? t.status) === "completed",
            ).length;

            return (
              <div
                key={group.key}
                className="rounded-lg border overflow-hidden"
              >
                {/* Batch header */}
                <div
                  className="flex items-center justify-between p-4 cursor-pointer hover:bg-accent/20 transition-colors"
                  onClick={() => toggleGroup(group.key)}
                >
                  <div className="flex items-center gap-2">
                    <span className="text-xs text-muted-foreground">
                      {isCollapsed ? "▸" : "▾"}
                    </span>
                    <span className="font-medium">{group.title}</span>
                    <span className="text-xs text-muted-foreground">
                      {completedCount}/{group.tasks.length} 个片段
                    </span>
                  </div>
                  <div className="flex items-center gap-2">
                    <span className={`text-sm ${batchLabel.color}`}>
                      {batchLabel.text}
                    </span>
                    {!["processing", "pending"].includes(batchStatus) && (
                      <Button
                        variant="ghost"
                        size="sm"
                        className="text-muted-foreground hover:text-red-500"
                        onClick={(e) => {
                          e.stopPropagation();
                          handleDeleteBatch(group.key, group.title);
                        }}
                      >
                        删除批次
                      </Button>
                    )}
                  </div>
                </div>

                {/* Batch progress bar */}
                {batchStatus === "processing" && (
                  <div className="px-4 pb-2">
                    <ProgressBar
                      progress={batchProgress}
                      message={`总进度 ${Math.round(batchProgress * 100)}%`}
                    />
                  </div>
                )}

                {/* Sub-items */}
                {!isCollapsed && (
                  <div className="border-t divide-y">
                    {group.tasks.map((task) => {
                      const live = liveProgress[task.id];
                      const status = live?.status ?? task.status;
                      const progress = live?.progress ?? task.progress;
                      const label =
                        statusLabels[status] ?? statusLabels.pending;
                      const duration =
                        task.end_time_ms - task.start_time_ms;

                      return (
                        <div
                          key={task.id}
                          className="px-4 py-2.5 space-y-1.5"
                        >
                          <div className="flex items-center justify-between">
                            <div className="text-sm">
                              <span className="font-medium">
                                {task.title ?? `片段 #${task.id}`}
                              </span>
                              <span className="ml-2 text-xs text-muted-foreground">
                                {formatTime(task.start_time_ms)} →{" "}
                                {formatTime(task.end_time_ms)}
                                <span className="ml-1">
                                  ({formatDurationMs(duration)})
                                </span>
                              </span>
                            </div>
                            <div className="flex items-center gap-2">
                              <span
                                className={`text-xs ${label.color}`}
                              >
                                {label.text}
                              </span>
                              {status === "processing" && (
                                <Button
                                  variant="ghost"
                                  size="sm"
                                  className="text-red-500 h-6 px-1.5 text-xs"
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    handleCancel(task.id);
                                  }}
                                >
                                  取消
                                </Button>
                              )}
                              {["completed", "failed", "cancelled"].includes(
                                status,
                              ) && (
                                <Button
                                  variant="ghost"
                                  size="sm"
                                  className="text-muted-foreground hover:text-red-500 h-6 px-1.5 text-xs"
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    handleDeleteTask(task.id);
                                  }}
                                >
                                  删除
                                </Button>
                              )}
                            </div>
                          </div>
                          {status === "processing" && (
                            <ProgressBar
                              progress={progress}
                              message={live?.message}
                              slim
                            />
                          )}
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
          })}
        </div>
      )}

      {/* Delete confirmation dialog */}
      <Dialog
        open={deleteDialog.open}
        onOpenChange={(open) =>
          setDeleteDialog((prev) => ({ ...prev, open }))
        }
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{deleteDialog.title}</DialogTitle>
            <DialogDescription>{deleteDialog.description}</DialogDescription>
          </DialogHeader>
          <div className="py-2">
            <label className="flex items-center gap-2 cursor-pointer select-none">
              <input
                type="checkbox"
                checked={deleteDialog.deleteFiles}
                onChange={(e) =>
                  setDeleteDialog((prev) => ({
                    ...prev,
                    deleteFiles: e.target.checked,
                  }))
                }
                className="h-4 w-4 rounded border-input"
              />
              <span className="text-sm">同时删除输出文件</span>
            </label>
            {deleteDialog.deleteFiles && (
              <p className="text-xs text-red-500 mt-1 ml-6">
                将从磁盘上永久删除已生成的文件，此操作不可撤销
              </p>
            )}
          </div>
          <DialogFooter>
            <DialogClose
              render={<Button variant="outline" />}
            >
              取消
            </DialogClose>
            <Button
              variant="destructive"
              onClick={handleDialogConfirm}
            >
              确认删除
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

/** Reusable progress bar */
function ProgressBar({
  progress,
  message,
  slim,
}: {
  progress: number;
  message?: string;
  slim?: boolean;
}) {
  return (
    <div className="space-y-1">
      <div
        className={`w-full bg-muted rounded-full overflow-hidden ${slim ? "h-1.5" : "h-2"}`}
      >
        <div
          className="h-full bg-primary transition-all duration-300"
          style={{ width: `${progress * 100}%` }}
        />
      </div>
      {message && (
        <div className="text-xs text-muted-foreground">{message}</div>
      )}
    </div>
  );
}

export const Route = createFileRoute("/dashboard/tasks")({
  component: TasksPage,
});
