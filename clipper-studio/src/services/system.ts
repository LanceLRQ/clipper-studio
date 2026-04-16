import { invoke } from "@tauri-apps/api/core";

/** Open a file with the system default application */
export const openFile = (path: string) => invoke("open_file", { path });

/** Reveal a file in the system file manager (Finder/Explorer) */
export const revealFile = (path: string) => invoke("reveal_file", { path });
