import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useEffect, useState, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import { open, ask } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { SearchIcon, FolderSyncIcon } from "lucide-react";
import type { ListVideosResponse, ListStreamersResponse } from "@/types/video";
import {
  listVideos,
  listStreamers,
  importVideo,
  deleteVideo,
} from "@/services/video";
import { useWorkspaceStore } from "@/stores/workspace";
import { scanWorkspace, getAppInfo } from "@/services/workspace";
import type { ScanResult } from "@/services/workspace";
import { ScanProgressCard } from "@/components/workspace/scan-progress-card";
import { mergeVideos } from "@/services/media";
import { VideoRow } from "@/components/video/video-row";
import { PaginationBar } from "@/components/video/pagination-bar";
import { StreamerCard, UnassociatedCard } from "@/components/video/streamer-card";
import { MergeToolbar } from "@/components/video/merge-toolbar";
import { TagFilter } from "@/components/tag/tag-filter";
import type { TagInfo } from "@/types/tag";
import { getVideoTags } from "@/services/tag";

interface VideoSearchParams {
  view?: "cards" | "flat";
  page?: number;
  search?: string;
}

const PAGE_SIZE = 50;

function VideosPage() {
  const navigate = useNavigate();
  const { view = "cards", page = 1, search = "" } = Route.useSearch();

  const [streamersData, setStreamersData] =
    useState<ListStreamersResponse | null>(null);
  const [flatVideos, setFlatVideos] = useState<ListVideosResponse | null>(null);
  const [unassociatedCount, setUnassociatedCount] = useState(0);
  const [loading, setLoading] = useState(true);
  const [importing, setImporting] = useState(false);
  const [selectedVideoIds, setSelectedVideoIds] = useState<Set<number>>(
    new Set()
  );
  const [mergeMode, setMergeMode] = useState<"virtual" | "physical">("virtual");
  const [merging, setMerging] = useState(false);
  const [scanning, setScanning] = useState(false);
  const [scanTaskId, setScanTaskId] = useState<number | null>(null);
  const [searchInput, setSearchInput] = useState(search);
  const [filterTagIds, setFilterTagIds] = useState<number[]>([]);
  const [videoTagsMap, setVideoTagsMap] = useState<Record<number, TagInfo[]>>({});

  const updateSearch = useCallback(
    (updates: Partial<VideoSearchParams>) => {
      navigate({
        to: "/dashboard/videos",
        search: (prev: VideoSearchParams) => ({
          ...prev,
          ...updates,
        }),
        replace: true,
      });
    },
    [navigate]
  );

  const activeWs = useWorkspaceStore((s) => s.activeId);
  const wsVersion = useWorkspaceStore((s) => s.version);
  const wsPathAccessible = useWorkspaceStore((s) => s.pathAccessible);
  const wsRecheckPath = useWorkspaceStore((s) => s.recheckPath);
  const [ffprobeAvailable, setFfprobeAvailable] = useState(true);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      if (view === "cards") {
        const [strs, unassoc] = await Promise.all([
          listStreamers({
            workspace_id: activeWs,
            page,
            page_size: PAGE_SIZE,
          }),
          // Count videos without streamer
          listVideos({
            workspace_id: activeWs,
            streamer_id: -1,
            page: 1,
            page_size: 1,
          }),
        ]);
        setStreamersData(strs);
        setUnassociatedCount(unassoc.total);
      } else {
        const flat = await listVideos({
          workspace_id: activeWs,
          page,
          page_size: PAGE_SIZE,
          search: search || undefined,
          tag_ids: filterTagIds.length > 0 ? filterTagIds : undefined,
        });
        setFlatVideos(flat);
      }
    } catch (e) {
      console.error("Failed to load videos:", e);
    } finally {
      setLoading(false);
    }
  }, [view, page, search, activeWs, wsVersion, filterTagIds]);

  useEffect(() => {
    loadData();
    wsRecheckPath();
    getAppInfo().then((info) => setFfprobeAvailable(info.ffprobe_available)).catch(console.error);
  }, [loadData, wsRecheckPath]);

  // Load tags for visible videos in flat view
  useEffect(() => {
    if (view !== "flat" || !flatVideos?.videos.length) return;
    const ids = flatVideos.videos.map((v) => v.id);
    Promise.all(ids.map((id) => getVideoTags(id).then((tags) => [id, tags] as const)))
      .then((results) => {
        const map: Record<number, TagInfo[]> = {};
        for (const [id, tags] of results) {
          if (tags.length > 0) map[id] = tags;
        }
        setVideoTagsMap(map);
      })
      .catch(console.error);
  }, [flatVideos, view]);

  // Auto-refresh when watcher detects new files
  useEffect(() => {
    let cancelled = false;
    let unlistenFn: (() => void) | undefined;

    listen("workspace-file-change", () => {
      loadData();
    }).then((fn) => {
      if (cancelled) { fn(); } else { unlistenFn = fn; }
    });

    return () => {
      cancelled = true;
      unlistenFn?.();
    };
  }, [loadData]);

  // Sync search input with URL
  useEffect(() => {
    setSearchInput(search);
  }, [search]);

  const handleSearchSubmit = () => {
    updateSearch({ search: searchInput || undefined, page: 1 });
  };

  const handleSearchKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") handleSearchSubmit();
  };

  const toggleVideoSelect = (id: number) => {
    setSelectedVideoIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const handleMerge = async () => {
    const ids = Array.from(selectedVideoIds);
    if (ids.length < 2) {
      alert("请至少选择 2 个视频");
      return;
    }
    if (
      !(await ask(
        `将合并 ${ids.length} 个视频（${mergeMode === "virtual" ? "快速合并" : "重编码合并"}），确认？`,
        { title: "合并视频" }
      ))
    )
      return;

    setMerging(true);
    try {
      await mergeVideos({ video_ids: ids, mode: mergeMode });
      setSelectedVideoIds(new Set());
      navigate({ to: "/dashboard/tasks" });
    } catch (e) {
      alert("合并失败: " + String(e));
    } finally {
      setMerging(false);
    }
  };

  const handleImport = async () => {
    try {
      const selected = await open({
        multiple: true,
        filters: [
          {
            name: "视频文件",
            extensions: [
              "mp4", "mkv", "flv", "ts", "avi", "mov", "wmv", "webm",
            ],
          },
        ],
      });
      if (!selected) return;

      setImporting(true);
      const paths = Array.isArray(selected) ? selected : [selected];
      for (const filePath of paths) {
        try {
          await importVideo({ file_path: filePath, workspace_id: activeWs ?? undefined });
        } catch (e) {
          console.error(`Failed to import ${filePath}:`, e);
        }
      }
      await loadData();
    } catch (e) {
      console.error("Import failed:", e);
    } finally {
      setImporting(false);
    }
  };

  const handleDelete = async (videoId: number, fileName: string, e: React.MouseEvent) => {
    e.stopPropagation();
    if (
      !(await ask(
        `确定要删除「${fileName}」吗？\n\n注意：仅删除记录，不会删除磁盘文件。`,
        { title: "删除视频", kind: "warning" }
      ))
    )
      return;
    try {
      await deleteVideo(videoId);
      await loadData();
    } catch (e) {
      console.error("Delete failed:", e);
    }
  };

  const navigateToVideo = (videoId: number) => {
    navigate({
      to: "/dashboard/videos/$videoId",
      params: { videoId: String(videoId) },
    });
  };

  const handleScan = async () => {
    if (!activeWs) return;
    setScanning(true);
    try {
      const taskId = await scanWorkspace(activeWs);
      setScanTaskId(taskId);
    } catch (e) {
      setScanning(false);
      alert("启动扫描失败: " + String(e));
    }
  };

  const handleScanComplete = useCallback(
    async (result: ScanResult | null) => {
      setScanTaskId(null);
      setScanning(false);
      await loadData();
      if (result) {
        alert(
          `扫描完成：新增 ${result.new_files} 个视频，共 ${result.total_files} 个，${result.total_sessions} 个场次`
        );
      } else {
        alert("扫描完成");
      }
    },
    [loadData]
  );

  const handleScanCancelled = useCallback(() => {
    setScanTaskId(null);
    setScanning(false);
  }, []);

  const handleScanFailed = useCallback((msg: string) => {
    setScanTaskId(null);
    setScanning(false);
    alert("扫描失败: " + msg);
  }, []);

  const totalItems =
    view === "cards"
      ? streamersData?.total ?? 0
      : flatVideos?.total ?? 0;

  return (
    <div className="space-y-4 p-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          {view === "cards" && streamersData && (
            <p className="text-sm text-muted-foreground">
              共 {streamersData.total} 个主播
              {unassociatedCount > 0 &&
                `，${unassociatedCount} 个未关联视频`}
            </p>
          )}
          {view === "flat" && flatVideos && (
            <p className="text-sm text-muted-foreground">
              共 {flatVideos.total} 个视频
            </p>
          )}
        </div>
        <div className="flex gap-2">
          {/* View mode toggle */}
          <div className="flex rounded-md border">
            {(
              [
                { key: "cards", label: "主播" },
                { key: "flat", label: "列表" },
              ] as const
            ).map(({ key, label }) => (
              <button
                key={key}
                className={`px-3 py-1 text-sm ${view === key ? "bg-accent" : ""}`}
                onClick={() => updateSearch({ view: key, page: 1, search: undefined })}
              >
                {label}
              </button>
            ))}
          </div>
          <Button
            variant="outline"
            onClick={handleScan}
            disabled={scanning || !wsPathAccessible || !ffprobeAvailable}
            title={!ffprobeAvailable ? "请先在「设置 > 依赖管理」中安装 FFmpeg" : !wsPathAccessible ? "工作区目录不可访问" : undefined}
          >
            <FolderSyncIcon className="h-4 w-4 mr-1" />
            {scanning ? "扫描中..." : "扫描目录"}
          </Button>
          <Button onClick={handleImport} disabled={importing || !wsPathAccessible}>
            {importing ? "导入中..." : "+ 导入视频"}
          </Button>
        </div>
      </div>

      {/* Scan progress card */}
      {scanTaskId !== null && (
        <ScanProgressCard
          taskId={scanTaskId}
          onComplete={handleScanComplete}
          onCancelled={handleScanCancelled}
          onFailed={handleScanFailed}
        />
      )}

      {/* Search bar (flat view only) */}
      {view === "flat" && (
        <div className="flex gap-2">
          <div className="relative flex-1 max-w-sm">
            <SearchIcon className="absolute left-2.5 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
            <Input
              value={searchInput}
              onChange={(e) => setSearchInput(e.target.value)}
              onKeyDown={handleSearchKeyDown}
              placeholder="搜索标题或文件名..."
              className="pl-8 h-8 text-sm"
            />
          </div>
          <Button variant="outline" size="sm" className="h-8" onClick={handleSearchSubmit}>
            搜索
          </Button>
          {search && (
            <Button
              variant="ghost"
              size="sm"
              className="h-8"
              onClick={() => {
                setSearchInput("");
                updateSearch({ search: undefined, page: 1 });
              }}
            >
              清除
            </Button>
          )}
          <TagFilter
            selectedTagIds={filterTagIds}
            onChange={(ids) => {
              setFilterTagIds(ids);
              updateSearch({ page: 1 });
            }}
          />
        </div>
      )}

      {/* Merge toolbar */}
      <MergeToolbar
        selectedCount={selectedVideoIds.size}
        mergeMode={mergeMode}
        onMergeModeChange={setMergeMode}
        onMerge={handleMerge}
        onCancel={() => setSelectedVideoIds(new Set())}
        merging={merging}
      />

      {loading ? (
        <div className="text-muted-foreground">加载中...</div>
      ) : totalItems === 0 && !search ? (
        <div className="rounded-lg border border-dashed p-12 text-center">
          <p className="text-muted-foreground mb-4">暂无视频</p>
          <Button onClick={handleImport}>导入视频文件</Button>
        </div>
      ) : view === "cards" ? (
        /* ===== Streamer card grid ===== */
        <div>
          <div className="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-4">
            {streamersData?.streamers.map((streamer) => (
              <StreamerCard key={streamer.id} streamer={streamer} />
            ))}
            <UnassociatedCard
              count={unassociatedCount}
              onClick={() =>
                navigate({
                  to: "/dashboard/videos/streamer/$streamerId",
                  params: { streamerId: "-1" },
                })
              }
            />
          </div>
          <PaginationBar
            page={page}
            pageSize={PAGE_SIZE}
            total={streamersData?.total ?? 0}
            onPageChange={(p) => updateSearch({ page: p })}
          />
        </div>
      ) : (
        /* ===== Flat list view ===== */
        <div>
          {flatVideos && flatVideos.videos.length === 0 && search && (
            <div className="rounded-lg border border-dashed p-8 text-center text-muted-foreground">
              没有找到匹配「{search}」的视频
            </div>
          )}
          <div className="space-y-2">
            {flatVideos?.videos.map((video) => (
              <VideoRow
                key={video.id}
                video={video}
                onNavigate={() => navigateToVideo(video.id)}
                onDelete={(e) =>
                  handleDelete(video.id, video.file_name, e)
                }
                selected={selectedVideoIds.has(video.id)}
                onToggleSelect={toggleVideoSelect}
                tags={videoTagsMap[video.id]}
              />
            ))}
          </div>
          <PaginationBar
            page={page}
            pageSize={PAGE_SIZE}
            total={flatVideos?.total ?? 0}
            onPageChange={(p) => updateSearch({ page: p })}
          />
        </div>
      )}
    </div>
  );
}

export const Route = createFileRoute("/dashboard/videos/")({
  component: VideosPage,
  validateSearch: (search: Record<string, unknown>): VideoSearchParams => ({
    view: (search.view as VideoSearchParams["view"]) ?? "cards",
    page: Number(search.page) || 1,
    search: (search.search as string) ?? "",
  }),
});
