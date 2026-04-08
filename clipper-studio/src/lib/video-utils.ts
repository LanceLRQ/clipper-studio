import type { VideoInfo } from "@/types/video";

export function formatDuration(ms: number | null): string {
  if (!ms) return "--:--";
  const totalSec = Math.floor(ms / 1000);
  const h = Math.floor(totalSec / 3600);
  const m = Math.floor((totalSec % 3600) / 60);
  const s = totalSec % 60;
  if (h > 0)
    return `${h}:${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

export function formatFileSize(bytes: number): string {
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

/** Compute end time from recorded_at + duration_ms */
export function computeEndTime(
  recordedAt: string | null,
  durationMs: number | null
): string | null {
  if (!recordedAt || !durationMs) return null;
  const match = recordedAt.match(
    /^(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2}):(\d{2})$/
  );
  if (!match) return null;
  const date = new Date(
    parseInt(match[1]),
    parseInt(match[2]) - 1,
    parseInt(match[3]),
    parseInt(match[4]),
    parseInt(match[5]),
    parseInt(match[6])
  );
  date.setMilliseconds(date.getMilliseconds() + durationMs);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

/** Format time range: show date once if same day */
export function formatTimeRange(
  start: string | null,
  end: string | null
): string {
  if (!start) return "";
  const startDate = start.slice(0, 10);
  const startTime = start.slice(11);
  if (!end) return start;
  const endDate = end.slice(0, 10);
  const endTime = end.slice(11);
  if (startDate === endDate) {
    return `${startDate} ${startTime} ~ ${endTime}`;
  }
  return `${start} ~ ${end}`;
}

/** Build display title: 主播 - 标题 - 时间段 */
export function buildVideoTitle(video: VideoInfo): string {
  const parts: string[] = [];
  if (video.streamer_name) parts.push(video.streamer_name);
  if (video.stream_title) parts.push(video.stream_title);
  const endTime = computeEndTime(video.recorded_at, video.duration_ms);
  const timeRange = formatTimeRange(video.recorded_at, endTime);
  if (timeRange) parts.push(timeRange);
  return parts.length > 0 ? parts.join(" - ") : video.file_name;
}

/** Format date string to Chinese: "2026年4月7日" */
export function formatDateHeader(dateStr: string): string {
  const match = dateStr.match(/^(\d{4})-(\d{2})-(\d{2})$/);
  if (!match) return dateStr;
  const y = parseInt(match[1]);
  const m = parseInt(match[2]);
  const d = parseInt(match[3]);
  return `${y}年${m}月${d}日`;
}

/** Get the Monday date string for the week containing a given date (ISO week) */
export function getWeekKey(dateStr: string): string {
  const match = dateStr.match(/^(\d{4})-(\d{2})-(\d{2})/);
  if (!match) return dateStr;
  const d = new Date(parseInt(match[1]), parseInt(match[2]) - 1, parseInt(match[3]));
  const day = d.getDay();
  // Shift to Monday-based: Monday=0, Sunday=6
  const diff = (day === 0 ? 6 : day - 1);
  d.setDate(d.getDate() - diff);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Get month key "yyyy-MM" from date string */
export function getMonthKey(dateStr: string): string {
  return dateStr.slice(0, 7);
}

/** Format week range header: "4月7日 - 4月13日" */
export function formatWeekHeader(mondayStr: string): string {
  const match = mondayStr.match(/^(\d{4})-(\d{2})-(\d{2})$/);
  if (!match) return mondayStr;
  const y = parseInt(match[1]);
  const m = parseInt(match[2]);
  const d = parseInt(match[3]);
  const monday = new Date(y, m - 1, d);
  const sunday = new Date(y, m - 1, d + 6);
  const fmtM = monday.getMonth() + 1;
  const fmtD = monday.getDate();
  const fmtM2 = sunday.getMonth() + 1;
  const fmtD2 = sunday.getDate();
  if (monday.getFullYear() !== sunday.getFullYear()) {
    return `${monday.getFullYear()}年${fmtM}月${fmtD}日 - ${sunday.getFullYear()}年${fmtM2}月${fmtD2}日`;
  }
  if (fmtM === fmtM2) {
    return `${fmtM}月${fmtD}日 - ${fmtD2}日`;
  }
  return `${fmtM}月${fmtD}日 - ${fmtM2}月${fmtD2}日`;
}

/** Format month header: "2026年4月" */
export function formatMonthHeader(monthKey: string): string {
  const match = monthKey.match(/^(\d{4})-(\d{2})$/);
  if (!match) return monthKey;
  return `${parseInt(match[1])}年${parseInt(match[2])}月`;
}
