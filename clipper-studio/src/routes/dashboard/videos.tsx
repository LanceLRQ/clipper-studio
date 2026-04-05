import { createFileRoute } from "@tanstack/react-router";

function VideosPage() {
  return (
    <div className="space-y-4">
      <h2 className="text-2xl font-semibold">视频列表</h2>
      <p className="text-muted-foreground">
        导入并管理你的录播视频文件。
      </p>
      <div className="rounded-lg border border-dashed p-12 text-center text-muted-foreground">
        暂无视频，点击导入按钮添加视频文件
      </div>
    </div>
  );
}

export const Route = createFileRoute("/dashboard/videos")({
  component: VideosPage,
});
