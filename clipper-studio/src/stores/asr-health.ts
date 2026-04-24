import { useEffect } from "react";
import { create } from "zustand";
import { checkASRHealth } from "@/services/asr";

/**
 * Remote ASR 健康状态共享 store。
 *
 * 背景（P5-PERF-25）：dashboard 状态栏与字幕面板各自 30s 轮询 checkASRHealth，
 * 同屏时请求数翻倍且时间点错开，浪费后端资源。此 store 用引用计数合并轮询：
 * 任意数量订阅者共享同一个 30s 定时器，最后一个卸载时停止。
 */

const POLL_INTERVAL_MS = 30_000;

interface ASRHealthState {
  /** true=就绪，false=不可达，null=未检测/未启用远程 */
  remoteHealthy: boolean | null;
  _refCount: number;
  _timer: ReturnType<typeof setInterval> | null;
  /** 标记当前是否启用了远程模式，mode=disabled/local 时不应该轮询 */
  _remoteEnabled: boolean;

  _poll: () => void;
  setRemoteEnabled: (enabled: boolean) => void;
  acquire: () => void;
  release: () => void;
}

export const useASRHealthStore = create<ASRHealthState>((set, get) => ({
  remoteHealthy: null,
  _refCount: 0,
  _timer: null,
  _remoteEnabled: false,

  _poll() {
    if (!get()._remoteEnabled) return;
    checkASRHealth()
      .then((h) => set({ remoteHealthy: h.status === "ready" }))
      .catch(() => set({ remoteHealthy: false }));
  },

  setRemoteEnabled(enabled) {
    const prev = get()._remoteEnabled;
    if (prev === enabled) return;
    set({ _remoteEnabled: enabled, remoteHealthy: enabled ? get().remoteHealthy : null });
    // 模式切换时，如果有订阅者则立刻发起一次检测
    if (enabled && get()._refCount > 0) {
      get()._poll();
    }
  },

  acquire() {
    const next = get()._refCount + 1;
    set({ _refCount: next });
    if (next === 1 && get()._timer === null) {
      // 立即触发一次，避免 30s 延迟
      get()._poll();
      const timer = setInterval(() => get()._poll(), POLL_INTERVAL_MS);
      set({ _timer: timer });
    }
  },

  release() {
    const next = Math.max(0, get()._refCount - 1);
    set({ _refCount: next });
    if (next === 0) {
      const t = get()._timer;
      if (t !== null) clearInterval(t);
      set({ _timer: null });
    }
  },
}));

/**
 * React Hook：订阅 ASR 远程健康状态。
 *
 * @param remoteMode 当前 ASR 模式是否为 remote；非 remote 时不会轮询，返回 null
 * @returns `remoteHealthy` — true/false/null（见 store 注释）
 */
export function useASRHealth(remoteMode: boolean): boolean | null {
  const remoteHealthy = useASRHealthStore((s) => s.remoteHealthy);
  const acquire = useASRHealthStore((s) => s.acquire);
  const release = useASRHealthStore((s) => s.release);
  const setRemoteEnabled = useASRHealthStore((s) => s.setRemoteEnabled);

  useEffect(() => {
    setRemoteEnabled(remoteMode);
  }, [remoteMode, setRemoteEnabled]);

  useEffect(() => {
    if (!remoteMode) return;
    acquire();
    return () => release();
  }, [remoteMode, acquire, release]);

  return remoteMode ? remoteHealthy : null;
}
