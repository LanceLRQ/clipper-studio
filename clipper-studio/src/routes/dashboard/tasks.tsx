import { createFileRoute } from "@tanstack/react-router";

function TasksPage() {
  return (
    <div className="space-y-4">
      <h2 className="text-2xl font-semibold">任务中心</h2>
      <p className="text-muted-foreground">
        查看和管理所有后台处理任务。
      </p>
      <div className="rounded-lg border border-dashed p-12 text-center text-muted-foreground">
        暂无任务
      </div>
    </div>
  );
}

export const Route = createFileRoute("/dashboard/tasks")({
  component: TasksPage,
});
