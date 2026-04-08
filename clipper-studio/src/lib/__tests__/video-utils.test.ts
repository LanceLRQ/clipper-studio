import { describe, it, expect } from "vitest";
import {
  formatDuration,
  formatFileSize,
  computeEndTime,
  formatTimeRange,
  buildVideoTitle,
  formatDateHeader,
  getWeekKey,
  getMonthKey,
  formatWeekHeader,
  formatMonthHeader,
} from "../video-utils";
import type { VideoInfo } from "@/types/video";

// ==================== formatDuration ====================

describe("formatDuration", () => {
  it("returns placeholder for null", () => {
    expect(formatDuration(null)).toBe("--:--");
  });

  it("returns placeholder for 0", () => {
    expect(formatDuration(0)).toBe("--:--");
  });

  it("formats seconds only", () => {
    expect(formatDuration(30_000)).toBe("0:30");
  });

  it("formats minutes and seconds", () => {
    expect(formatDuration(90_000)).toBe("1:30");
  });

  it("formats hours, minutes, seconds", () => {
    expect(formatDuration(3_661_000)).toBe("1:01:01");
  });
});

// ==================== formatFileSize ====================

describe("formatFileSize", () => {
  it("formats KB", () => {
    expect(formatFileSize(512)).toBe("0.5 KB");
    expect(formatFileSize(1024)).toBe("1.0 KB");
  });

  it("formats MB", () => {
    expect(formatFileSize(1024 * 1024)).toBe("1.0 MB");
    expect(formatFileSize(512 * 1024 * 1024)).toBe("512.0 MB");
  });

  it("formats GB", () => {
    expect(formatFileSize(1024 * 1024 * 1024)).toBe("1.00 GB");
  });
});

// ==================== computeEndTime ====================

describe("computeEndTime", () => {
  it("returns null for null inputs", () => {
    expect(computeEndTime(null, 1000)).toBeNull();
    expect(computeEndTime("2026-04-07 20:00:00", null)).toBeNull();
  });

  it("computes end time correctly", () => {
    const result = computeEndTime("2026-04-07 20:00:00", 3600_000);
    expect(result).toBe("2026-04-07 21:00:00");
  });

  it("returns null for invalid format", () => {
    expect(computeEndTime("invalid", 1000)).toBeNull();
  });
});

// ==================== formatTimeRange ====================

describe("formatTimeRange", () => {
  it("returns empty for null start", () => {
    expect(formatTimeRange(null, null)).toBe("");
  });

  it("returns start only when no end", () => {
    expect(formatTimeRange("2026-04-07 20:00:00", null)).toBe(
      "2026-04-07 20:00:00"
    );
  });

  it("collapses same day", () => {
    expect(formatTimeRange("2026-04-07 20:00:00", "2026-04-07 22:00:00")).toBe(
      "2026-04-07 20:00:00 ~ 22:00:00"
    );
  });

  it("shows both dates when different day", () => {
    expect(formatTimeRange("2026-04-07 23:00:00", "2026-04-08 01:00:00")).toBe(
      "2026-04-07 23:00:00 ~ 2026-04-08 01:00:00"
    );
  });
});

// ==================== buildVideoTitle ====================

describe("buildVideoTitle", () => {
  it("builds title with streamer, stream_title, time range", () => {
    const video = {
      streamer_name: "主播A",
      stream_title: "测试直播",
      recorded_at: "2026-04-07 20:00:00",
      duration_ms: 3600_000,
      file_name: "video.mp4",
    } as VideoInfo;
    const result = buildVideoTitle(video);
    expect(result).toContain("主播A");
    expect(result).toContain("测试直播");
    expect(result).toContain("20:00:00 ~ 21:00:00");
  });

  it("falls back to file_name when no metadata", () => {
    const video = {
      file_name: "test.flv",
    } as VideoInfo;
    expect(buildVideoTitle(video)).toBe("test.flv");
  });
});

// ==================== formatDateHeader ====================

describe("formatDateHeader", () => {
  it("formats Chinese date", () => {
    expect(formatDateHeader("2026-04-07")).toBe("2026年4月7日");
  });

  it("returns raw string for invalid format", () => {
    expect(formatDateHeader("invalid")).toBe("invalid");
  });
});

// ==================== getWeekKey ====================

describe("getWeekKey", () => {
  it("returns Monday of the week", () => {
    // 2026-04-07 is a Tuesday, Monday is 2026-04-06
    expect(getWeekKey("2026-04-07")).toBe("2026-04-06");
  });

  it("returns Monday for Sunday", () => {
    // 2026-04-12 is a Sunday, Monday is 2026-04-06
    expect(getWeekKey("2026-04-12")).toBe("2026-04-06");
  });

  it("returns self for Monday", () => {
    // 2026-04-06 is a Monday
    expect(getWeekKey("2026-04-06")).toBe("2026-04-06");
  });
});

// ==================== getMonthKey ====================

describe("getMonthKey", () => {
  it("extracts year-month", () => {
    expect(getMonthKey("2026-04-07")).toBe("2026-04");
  });
});

// ==================== formatWeekHeader ====================

describe("formatWeekHeader", () => {
  it("formats same month range", () => {
    expect(formatWeekHeader("2026-04-06")).toBe("4月6日 - 12日");
  });

  it("formats cross-month range", () => {
    expect(formatWeekHeader("2026-03-30")).toBe("3月30日 - 4月5日");
  });

  it("returns raw string for invalid input", () => {
    expect(formatWeekHeader("invalid")).toBe("invalid");
  });
});

// ==================== formatMonthHeader ====================

describe("formatMonthHeader", () => {
  it("formats month in Chinese", () => {
    expect(formatMonthHeader("2026-04")).toBe("2026年4月");
  });

  it("returns raw string for invalid input", () => {
    expect(formatMonthHeader("invalid")).toBe("invalid");
  });
});
