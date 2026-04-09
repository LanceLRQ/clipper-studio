import { useState, useEffect, useRef, useCallback } from "react";
import { useNavigate } from "@tanstack/react-router";
import {
  Dialog,
  DialogContent,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  searchSubtitlesGlobal,
  type SubtitleSearchResult,
} from "@/services/asr";
import { Search, FileVideo, Loader2 } from "lucide-react";

// ===== Helpers =====

function formatTimecode(ms: number): string {
  const totalSecs = Math.floor(ms / 1000);
  const h = Math.floor(totalSecs / 3600);
  const m = Math.floor((totalSecs % 3600) / 60);
  const s = totalSecs % 60;
  if (h > 0) {
    return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }
  return `${m}:${String(s).padStart(2, "0")}`;
}

/**
 * Parse "yyyy-MM-dd HH:mm:ss" to ms using simplified calculation.
 * Must match Rust parse_recorded_at_to_unix_ms exactly (y*365 + mo*30 + d).
 */
function parseRecordedAtMs(recorded_at: string | null): number | null {
  if (!recorded_at) return null;
  const parts = recorded_at.split(/[-\s:]/);
  if (parts.length < 6) return null;
  const [y, mo, d, h, mi, sec] = parts.map(Number);
  if (isNaN(y)) return null;
  const days = y * 365 + mo * 30 + d;
  const secs = days * 86400 + h * 3600 + mi * 60 + sec;
  return secs * 1000;
}

/** Compute file-relative ms from absolute ms */
function toRelativeMs(
  absoluteMs: number,
  recordedAt: string | null
): number {
  const baseMs = parseRecordedAtMs(recordedAt);
  if (baseMs != null && absoluteMs > baseMs) {
    return absoluteMs - baseMs;
  }
  // Fallback: already relative
  return absoluteMs;
}

/** Group search results by video_id */
function groupByVideo(
  results: SubtitleSearchResult[]
): Map<number, SubtitleSearchResult[]> {
  const map = new Map<number, SubtitleSearchResult[]>();
  for (const r of results) {
    const list = map.get(r.video_id);
    if (list) {
      list.push(r);
    } else {
      map.set(r.video_id, [r]);
    }
  }
  return map;
}

/** Highlight matching text */
function HighlightText({
  text,
  query,
}: {
  text: string;
  query: string;
}) {
  if (!query.trim()) return <>{text}</>;

  const escaped = query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const regex = new RegExp(`(${escaped})`, "gi");
  const parts = text.split(regex);

  return (
    <>
      {parts.map((part, i) =>
        regex.test(part) ? (
          <mark
            key={i}
            className="bg-yellow-200 dark:bg-yellow-800 text-foreground rounded-sm px-0.5"
          >
            {part}
          </mark>
        ) : (
          <span key={i}>{part}</span>
        )
      )}
    </>
  );
}

// ===== Component =====

interface GlobalSearchDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function GlobalSearchDialog({
  open,
  onOpenChange,
}: GlobalSearchDialogProps) {
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const [debouncedQuery, setDebouncedQuery] = useState("");
  const [results, setResults] = useState<SubtitleSearchResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [searched, setSearched] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout>>();

  // Reset state when dialog closes
  useEffect(() => {
    if (!open) {
      setQuery("");
      setDebouncedQuery("");
      setResults([]);
      setSearched(false);
      setLoading(false);
    }
  }, [open]);

  // Debounce query input
  useEffect(() => {
    if (timerRef.current) clearTimeout(timerRef.current);
    if (!query.trim()) {
      setDebouncedQuery("");
      setResults([]);
      setSearched(false);
      return;
    }
    timerRef.current = setTimeout(() => {
      setDebouncedQuery(query.trim());
    }, 300);
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [query]);

  // Execute search
  useEffect(() => {
    if (!debouncedQuery) return;

    let cancelled = false;
    setLoading(true);

    searchSubtitlesGlobal(debouncedQuery)
      .then((data) => {
        if (!cancelled) {
          setResults(data);
          setSearched(true);
        }
      })
      .catch((e) => {
        if (!cancelled) {
          console.error("Search failed:", e);
          setResults([]);
          setSearched(true);
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [debouncedQuery]);

  const handleResultClick = useCallback(
    (result: SubtitleSearchResult) => {
      const relativeMs = toRelativeMs(result.start_ms, result.recorded_at);
      const seconds = Math.floor(relativeMs / 1000);
      onOpenChange(false);
      navigate({
        to: "/dashboard/videos/$videoId",
        params: { videoId: String(result.video_id) },
        search: { t: seconds },
      });
    },
    [navigate, onOpenChange]
  );

  const grouped = groupByVideo(results);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="sm:max-w-xl max-h-[70vh] flex flex-col p-0 gap-0"
        showCloseButton={false}
      >
        <DialogTitle className="sr-only">全局字幕搜索</DialogTitle>

        {/* Search Input */}
        <div className="flex items-center gap-2 border-b px-4 py-3">
          <Search className="h-4 w-4 text-muted-foreground shrink-0" />
          <Input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="搜索所有视频字幕..."
            className="border-0 shadow-none focus-visible:ring-0 h-8 text-sm"
            autoFocus
          />
          {loading && (
            <Loader2 className="h-4 w-4 text-muted-foreground animate-spin shrink-0" />
          )}
          <kbd className="hidden sm:inline-flex h-5 items-center rounded border bg-muted px-1.5 text-[10px] font-medium text-muted-foreground">
            ESC
          </kbd>
        </div>

        {/* Results */}
        <div className="flex-1 overflow-y-auto p-2 min-h-[200px] max-h-[calc(70vh-60px)]">
          {!debouncedQuery && !searched && (
            <div className="flex flex-col items-center justify-center h-full text-muted-foreground text-sm py-12">
              <Search className="h-8 w-8 mb-3 opacity-30" />
              <p>输入关键词搜索所有视频字幕</p>
              <p className="text-xs mt-1 opacity-60">
                支持跨视频全文搜索，点击结果跳转到对应时间点
              </p>
            </div>
          )}

          {searched && results.length === 0 && !loading && (
            <div className="flex flex-col items-center justify-center h-full text-muted-foreground text-sm py-12">
              <p>未找到匹配结果</p>
              <p className="text-xs mt-1 opacity-60">
                试试换个关键词？
              </p>
            </div>
          )}

          {grouped.size > 0 && (
            <div className="space-y-3">
              {Array.from(grouped.entries()).map(
                ([videoId, segments]) => {
                  const first = segments[0];
                  const videoLabel =
                    first.streamer_name && first.stream_title
                      ? `[${first.streamer_name}] ${first.stream_title}`
                      : first.stream_title || first.video_file_name;

                  return (
                    <div key={videoId}>
                      {/* Video Group Header */}
                      <div className="flex items-center gap-1.5 px-2 py-1 text-xs text-muted-foreground">
                        <FileVideo className="h-3 w-3 shrink-0" />
                        <span className="truncate font-medium">
                          {videoLabel}
                        </span>
                        <span className="shrink-0 opacity-60">
                          ({segments.length}条)
                        </span>
                      </div>

                      {/* Subtitle Results */}
                      <div className="space-y-0.5">
                        {segments.map((seg) => {
                          const relMs = toRelativeMs(
                            seg.start_ms,
                            seg.recorded_at
                          );
                          return (
                            <button
                              key={seg.id}
                              className="w-full text-left px-2 py-1.5 rounded-md hover:bg-accent transition-colors flex items-start gap-2 text-sm group"
                              onClick={() => handleResultClick(seg)}
                            >
                              <span className="text-xs text-muted-foreground font-mono shrink-0 pt-0.5 w-14 text-right">
                                {formatTimecode(relMs)}
                              </span>
                              <span className="flex-1 min-w-0 break-words">
                                <HighlightText
                                  text={seg.text}
                                  query={debouncedQuery}
                                />
                              </span>
                            </button>
                          );
                        })}
                      </div>
                    </div>
                  );
                }
              )}

              {results.length >= 200 && (
                <p className="text-center text-xs text-muted-foreground py-2">
                  仅显示前 200 条结果，请尝试更精确的关键词
                </p>
              )}
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
