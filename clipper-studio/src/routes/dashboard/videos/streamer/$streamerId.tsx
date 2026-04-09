import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useEffect, useState, useCallback, useMemo, useRef } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  ArrowLeftIcon,
  SearchIcon,
  ArrowUpDownIcon,
  ChevronDownIcon,
  ChevronRightIcon,
} from "lucide-react";
import { DateRangePicker } from "@/components/video/date-range-picker";
import type {
  SessionInfo,
  ListSessionsResponse,
  StreamerInfo,
} from "@/types/video";
import { listSessions, listStreamers, deleteVideo } from "@/services/video";
import { getActiveWorkspace } from "@/services/workspace";
import { mergeVideos } from "@/services/media";
import { VideoRow } from "@/components/video/video-row";
import { PaginationBar } from "@/components/video/pagination-bar";
import { MergeToolbar } from "@/components/video/merge-toolbar";
import {
  formatDuration,
  formatTimeRange,
  computeEndTime,
  getWeekKey,
  getMonthKey,
  formatWeekHeader,
  formatMonthHeader,
} from "@/lib/video-utils";

type GroupMode = "week" | "month";

interface StreamerSearchParams {
  page?: number;
  sort?: "asc" | "desc";
  group?: GroupMode;
  search?: string;
  dateFrom?: string;
  dateTo?: string;
}

const PAGE_SIZE = 50;

// sessionStorage helpers
function getStorageKey(streamerId: string, suffix: string) {
  return `streamer-${streamerId}-${suffix}`;
}

function saveExpanded(streamerId: string, expanded: Set<number>) {
  sessionStorage.setItem(
    getStorageKey(streamerId, "expanded"),
    JSON.stringify([...expanded])
  );
}

function loadExpanded(streamerId: string): Set<number> {
  try {
    const raw = sessionStorage.getItem(
      getStorageKey(streamerId, "expanded")
    );
    if (raw) return new Set(JSON.parse(raw) as number[]);
  } catch {
    // ignore
  }
  return new Set();
}

function saveScrollPos(streamerId: string, pos: number) {
  sessionStorage.setItem(
    getStorageKey(streamerId, "scroll"),
    String(pos)
  );
}

function loadScrollPos(streamerId: string): number {
  return Number(
    sessionStorage.getItem(getStorageKey(streamerId, "scroll")) ?? 0
  );
}

function StreamerVideosPage() {
  const navigate = useNavigate();
  const { streamerId } = Route.useParams();
  const {
    page = 1,
    sort = "desc",
    group = "week" as GroupMode,
    search = "",
    dateFrom = "",
    dateTo = "",
  } = Route.useSearch();

  const [sessionsData, setSessionsData] =
    useState<ListSessionsResponse | null>(null);
  const [streamerInfo, setStreamerInfo] = useState<StreamerInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [expandedSessions, setExpandedSessions] = useState<Set<number>>(
    () => loadExpanded(streamerId)
  );
  const [selectedVideoIds, setSelectedVideoIds] = useState<Set<number>>(
    new Set()
  );
  const [mergeMode, setMergeMode] = useState<"virtual" | "physical">("virtual");
  const [merging, setMerging] = useState(false);
  const [searchInput, setSearchInput] = useState(search);
  const scrollRef = useRef<HTMLDivElement>(null);
  const scrollRestored = useRef(false);

  const sid = Number(streamerId);

  // Persist expanded state to sessionStorage
  useEffect(() => {
    saveExpanded(streamerId, expandedSessions);
  }, [streamerId, expandedSessions]);

  // Save scroll position continuously
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const handleScroll = () => {
      saveScrollPos(streamerId, el.scrollTop);
    };
    el.addEventListener("scroll", handleScroll, { passive: true });
    return () => el.removeEventListener("scroll", handleScroll);
  }, [streamerId]);

  const updateSearch = useCallback(
    (updates: Partial<StreamerSearchParams>) => {
      navigate({
        to: "/dashboard/videos/streamer/$streamerId",
        params: { streamerId },
        search: (prev: StreamerSearchParams) => ({
          ...prev,
          ...updates,
        }),
        replace: true,
      });
    },
    [navigate, streamerId]
  );

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const activeWs = await getActiveWorkspace();

      if (sid > 0) {
        const strs = await listStreamers({ workspace_id: activeWs });
        const found = strs.streamers.find((s) => s.id === sid);
        setStreamerInfo(found ?? null);
      }

      const resp = await listSessions({
        workspace_id: activeWs,
        streamer_id: sid,
        sort_order: sort,
        search: search || undefined,
        date_from: dateFrom || undefined,
        date_to: dateTo || undefined,
        page,
        page_size: PAGE_SIZE,
      });
      setSessionsData(resp);
    } catch (e) {
      console.error("Failed to load streamer sessions:", e);
    } finally {
      setLoading(false);
    }
  }, [sid, page, sort, search, dateFrom, dateTo]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // Restore scroll position after data loads
  useEffect(() => {
    if (!loading && sessionsData && !scrollRestored.current) {
      scrollRestored.current = true;
      const pos = loadScrollPos(streamerId);
      if (pos > 0 && scrollRef.current) {
        requestAnimationFrame(() => {
          scrollRef.current?.scrollTo(0, pos);
        });
      }
    }
  }, [loading, sessionsData, streamerId]);

  useEffect(() => {
    setSearchInput(search);
  }, [search]);

  // Group sessions by week or month
  const groupedSessions = useMemo(() => {
    if (!sessionsData) return [];
    const groups = new Map<string, SessionInfo[]>();
    for (const s of sessionsData.sessions) {
      const dateStr = s.started_at?.slice(0, 10) ?? "";
      let key: string;
      if (!dateStr) {
        key = "未知时间";
      } else if (group === "week") {
        key = getWeekKey(dateStr);
      } else {
        key = getMonthKey(dateStr);
      }
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key)!.push(s);
    }
    return Array.from(groups.entries());
  }, [sessionsData, group]);

  const formatGroupHeader = (key: string): string => {
    if (key === "未知时间") return key;
    return group === "week" ? formatWeekHeader(key) : formatMonthHeader(key);
  };

  const toggleSession = (sessionId: number) => {
    setExpandedSessions((prev) => {
      const next = new Set(prev);
      if (next.has(sessionId)) next.delete(sessionId);
      else next.add(sessionId);
      return next;
    });
  };

  const handleSearchSubmit = () => {
    updateSearch({ search: searchInput || undefined, page: 1 });
  };

  const handleSearchKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") handleSearchSubmit();
  };

  const toggleSort = () => {
    updateSearch({ sort: sort === "desc" ? "asc" : "desc", page: 1 });
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
    if (ids.length < 2) return;
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

  const handleDelete = async (
    videoId: number,
    fileName: string,
    e: React.MouseEvent
  ) => {
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

  const streamerName =
    sid === -1 ? "未关联主播" : streamerInfo?.name ?? "加载中...";

  return (
    <div className="flex flex-col h-full p-6">
      {/* Fixed header area */}
      <div className="shrink-0 space-y-3 pb-3 relative z-10">
        {/* Title bar */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={() => window.history.back()}
            >
              <ArrowLeftIcon className="h-4 w-4" />
            </Button>
            <div>
              <h2 className="text-2xl font-semibold">{streamerName}</h2>
              <p className="text-sm text-muted-foreground">
                {sessionsData ? `${sessionsData.total} 个场次` : ""}
                {streamerInfo?.total_duration_ms
                  ? ` · 总时长 ${formatDuration(streamerInfo.total_duration_ms)}`
                  : ""}
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            {/* Group mode toggle */}
            <div className="flex rounded-md border">
              {(
                [
                  { key: "week", label: "按周" },
                  { key: "month", label: "按月" },
                ] as const
              ).map(({ key, label }) => (
                <button
                  key={key}
                  className={`px-2.5 py-1 text-xs ${group === key ? "bg-accent" : ""}`}
                  onClick={() => updateSearch({ group: key })}
                >
                  {label}
                </button>
              ))}
            </div>
            <Button variant="outline" size="sm" onClick={toggleSort}>
              <ArrowUpDownIcon className="h-4 w-4 mr-1" />
              {sort === "desc" ? "最新优先" : "最早优先"}
            </Button>
          </div>
        </div>

        {/* Search + date filter */}
        <div className="flex items-center gap-2">
          <div className="relative flex-1 max-w-sm">
            <SearchIcon className="absolute left-2.5 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
            <Input
              value={searchInput}
              onChange={(e) => setSearchInput(e.target.value)}
              onKeyDown={handleSearchKeyDown}
              placeholder="搜索场次标题..."
              className="pl-8 h-8 text-sm"
            />
          </div>
          <Button variant="outline" size="sm" onClick={handleSearchSubmit}>
            搜索
          </Button>
          {search && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                setSearchInput("");
                updateSearch({ search: undefined, page: 1 });
              }}
            >
              清除搜索
            </Button>
          )}
          <div className="flex-1" />
          <DateRangePicker
            dateFrom={dateFrom}
            dateTo={dateTo}
            onChange={(from, to) =>
              updateSearch({ dateFrom: from, dateTo: to, page: 1 })
            }
          />
        </div>

        {/* Merge toolbar */}
        <MergeToolbar
          selectedCount={selectedVideoIds.size}
          mergeMode={mergeMode}
          onMergeModeChange={setMergeMode}
          onMerge={handleMerge}
          onCancel={() => setSelectedVideoIds(new Set())}
          merging={merging}
        />
      </div>

      {/* Scrollable session list */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto min-h-0">
        {loading ? (
          <div className="text-muted-foreground p-4">加载中...</div>
        ) : sessionsData && sessionsData.sessions.length === 0 ? (
          <div className="rounded-lg border border-dashed p-8 text-center text-muted-foreground">
            {search || dateFrom || dateTo
              ? "没有找到匹配的场次"
              : "暂无场次"}
          </div>
        ) : (
          <div className="space-y-1">
            {groupedSessions.map(([key, dateSessions]) => (
              <div key={key}>
                {/* Period header */}
                <div className="bg-background/95 backdrop-blur px-2 py-1.5 text-sm font-medium text-muted-foreground border-b">
                  {formatGroupHeader(key)}
                  <span className="ml-2 text-xs font-normal">
                    {dateSessions.length} 个场次
                  </span>
                </div>

                {/* Sessions under this date */}
                <div className="space-y-1 py-1">
                  {dateSessions.map((session) => {
                    const isExpanded = expandedSessions.has(session.id);
                    const sessDuration = session.videos.reduce(
                      (sum, v) => sum + (v.duration_ms ?? 0),
                      0
                    );
                    const endTime = computeEndTime(
                      session.started_at,
                      sessDuration
                    );
                    const timeRange = formatTimeRange(
                      session.started_at,
                      endTime
                    );

                    return (
                      <div
                        key={session.id}
                        className="rounded-lg border overflow-hidden"
                      >
                        {/* Session header */}
                        <div
                          className="flex items-center justify-between p-3 cursor-pointer hover:bg-accent/20 transition-colors"
                          onClick={() => toggleSession(session.id)}
                        >
                          <div className="flex items-center gap-2 min-w-0">
                            {isExpanded ? (
                              <ChevronDownIcon className="h-4 w-4 text-muted-foreground shrink-0" />
                            ) : (
                              <ChevronRightIcon className="h-4 w-4 text-muted-foreground shrink-0" />
                            )}
                            <div className="min-w-0">
                              <div className="text-sm font-medium truncate">
                                {session.title || "未命名场次"}
                              </div>
                              <div className="flex gap-3 text-xs text-muted-foreground">
                                {timeRange && <span>{timeRange}</span>}
                                <span>
                                  {session.videos.length} 个分片
                                </span>
                                <span>
                                  {formatDuration(sessDuration)}
                                </span>
                              </div>
                            </div>
                          </div>
                        </div>

                        {/* Expanded: videos in session */}
                        {isExpanded && (
                          <div className="border-t bg-muted/10">
                            {session.videos.map((video) => (
                              <VideoRow
                                key={video.id}
                                video={video}
                                compact
                                onNavigate={() =>
                                  navigateToVideo(video.id)
                                }
                                onDelete={(e) =>
                                  handleDelete(
                                    video.id,
                                    video.file_name,
                                    e
                                  )
                                }
                                selected={selectedVideoIds.has(video.id)}
                                onToggleSelect={toggleVideoSelect}
                              />
                            ))}
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              </div>
            ))}
          </div>
        )}

        {/* Pagination */}
        <PaginationBar
          page={page}
          pageSize={PAGE_SIZE}
          total={sessionsData?.total ?? 0}
          onPageChange={(p) => updateSearch({ page: p })}
        />
      </div>
    </div>
  );
}

export const Route = createFileRoute(
  "/dashboard/videos/streamer/$streamerId"
)({
  component: StreamerVideosPage,
  validateSearch: (
    search: Record<string, unknown>
  ): StreamerSearchParams => ({
    page: Number(search.page) || 1,
    sort: (search.sort as "asc" | "desc") ?? "desc",
    group: (search.group as GroupMode) ?? "week",
    search: (search.search as string) ?? "",
    dateFrom: (search.dateFrom as string) ?? "",
    dateTo: (search.dateTo as string) ?? "",
  }),
});
