import { useMemo } from "react";
import { create } from "zustand";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  type ASRQueueItem,
  type ASRTaskProgressEvent,
  submitASRQueued,
  cancelASRTask,
  getASRQueueSnapshot,
} from "@/services/asr";

/** Active statuses that represent in-progress tasks */
const ACTIVE_STATUSES = new Set(["queued", "converting", "submitting", "processing"]);
/** Terminal statuses that should be auto-cleaned from the store */
const TERMINAL_STATUSES = new Set(["completed", "failed", "cancelled"]);
/** Delay before removing terminal tasks from store (ms) */
const CLEANUP_DELAY_MS = 10_000;

interface ASRQueueState {
  /** Map of taskId -> queue item */
  tasks: Record<number, ASRQueueItem>;
  /** Whether the store has been initialized */
  initialized: boolean;

  /** Initialize the store: load snapshot + subscribe to events */
  initialize: () => Promise<void>;
  /** Submit a new ASR task to the queue */
  submitTask: (videoId: number, language?: string) => Promise<number>;
  /** Cancel an ASR task */
  cancelTask: (taskId: number) => Promise<void>;
}

let _unlisten: UnlistenFn | undefined;
const _cleanupTimers: Record<number, ReturnType<typeof setTimeout>> = {};

export const useASRQueueStore = create<ASRQueueState>((set, get) => ({
  tasks: {},
  initialized: false,

  async initialize() {
    if (get().initialized) return;

    // Load initial snapshot from backend
    try {
      const snapshot = await getASRQueueSnapshot();
      const tasks: Record<number, ASRQueueItem> = {};
      for (const item of snapshot) {
        tasks[item.task_id] = item;
      }
      set({ tasks, initialized: true });
    } catch (e) {
      console.error("Failed to load ASR queue snapshot:", e);
      set({ initialized: true });
    }

    // Subscribe to progress events
    if (_unlisten) return;
    _unlisten = await listen<ASRTaskProgressEvent>("asr-task-progress", (event) => {
      const p = event.payload;
      const item: ASRQueueItem = {
        task_id: p.task_id,
        video_id: p.video_id,
        video_file_name: p.video_file_name,
        status: p.status,
        progress: p.progress,
        message: p.message,
        error_message: p.error_message,
      };

      set((state) => ({
        tasks: { ...state.tasks, [p.task_id]: item },
      }));

      // Schedule cleanup for terminal tasks
      if (TERMINAL_STATUSES.has(p.status)) {
        if (_cleanupTimers[p.task_id]) {
          clearTimeout(_cleanupTimers[p.task_id]);
        }
        _cleanupTimers[p.task_id] = setTimeout(() => {
          set((state) => {
            const { [p.task_id]: _, ...rest } = state.tasks;
            return { tasks: rest };
          });
          delete _cleanupTimers[p.task_id];
        }, CLEANUP_DELAY_MS);
      }
    });
  },

  async submitTask(videoId: number, language?: string) {
    return submitASRQueued(videoId, language);
  },

  async cancelTask(taskId: number) {
    await cancelASRTask(taskId);
  },
}));

// ==================== Derived hooks (memo-cached, safe for React) ====================

/** Get the active ASR task for a specific video */
export function useASRTaskForVideo(videoId: number): ASRQueueItem | undefined {
  const tasks = useASRQueueStore((s) => s.tasks);
  return useMemo(() => {
    let terminal: ASRQueueItem | undefined;
    for (const item of Object.values(tasks)) {
      if (item.video_id === videoId) {
        if (ACTIVE_STATUSES.has(item.status)) return item;
        terminal = item;
      }
    }
    return terminal;
  }, [tasks, videoId]);
}

/** Get all active (non-terminal) tasks */
export function useASRActiveTasks(): ASRQueueItem[] {
  const tasks = useASRQueueStore((s) => s.tasks);
  return useMemo(
    () => Object.values(tasks).filter((t) => ACTIVE_STATUSES.has(t.status)),
    [tasks],
  );
}

/** Get count of active tasks */
export function useASRActiveCount(): number {
  const tasks = useASRQueueStore((s) => s.tasks);
  return useMemo(
    () => Object.values(tasks).filter((t) => ACTIVE_STATUSES.has(t.status)).length,
    [tasks],
  );
}

/** Get the currently running task (if any) */
export function useASRRunningTask(): ASRQueueItem | undefined {
  const tasks = useASRQueueStore((s) => s.tasks);
  return useMemo(
    () => Object.values(tasks).find(
      (t) => t.status === "converting" || t.status === "submitting" || t.status === "processing",
    ),
    [tasks],
  );
}
