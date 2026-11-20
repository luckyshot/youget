import type { FullState, Item, ItemOptions, Settings, ToolsState } from "./types";

/** True when running inside the Tauri shell (false in a plain browser preview). */
export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri) {
    const { mock } = await import("./mock");
    return mock.invoke(cmd, args) as Promise<T>;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

export async function on<T>(event: string, cb: (payload: T) => void): Promise<() => void> {
  if (!inTauri) {
    const { mock } = await import("./mock");
    return mock.listen(event, cb as (p: unknown) => void);
  }
  const { listen } = await import("@tauri-apps/api/event");
  return listen<T>(event, (e) => cb(e.payload));
}

export const api = {
  getState: () => call<FullState>("get_state"),
  addUrls: (urls: string[], opts: ItemOptions) => call<void>("add_urls", { urls, opts }),
  saveSettings: (settings: Settings) => call<void>("save_settings", { settings }),
  itemAction: (id: string, action: "pause" | "resume" | "cancel" | "retry" | "remove") =>
    call<void>("item_action", { id, action }),
  setItemOptions: (id: string, opts: ItemOptions) => call<void>("set_item_options", { id, opts }),
  setPendingOptions: (opts: ItemOptions) => call<void>("set_pending_options", { opts }),
  reorder: (ids: string[]) => call<void>("reorder", { ids }),
  clearFinished: () => call<void>("clear_finished"),
  pauseAll: () => call<void>("pause_all"),
  resumeAll: () => call<void>("resume_all"),
  getLog: (id: string) => call<string[]>("get_log", { id }),
  commandLine: (id: string) => call<string | null>("command_line", { id }),
  listFormats: (url: string) => call<{ title: string; duration: number; formats: import("./types").Format[] }>("list_formats", { url }),
  updateYtdlp: () => call<string>("update_ytdlp"),
  recheckTools: () => call<void>("recheck_tools"),
  reveal: (id: string) => call<void>("reveal", { id }),
  openFile: (id: string) => call<void>("open_file", { id }),
  openDir: (path: string) => call<void>("open_dir", { path }),
  async pickFolder(current: string): Promise<string | null> {
    if (!inTauri) return null;
    const { open } = await import("@tauri-apps/plugin-dialog");
    const r = await open({ directory: true, defaultPath: current || undefined, title: "Choose download folder" });
    return typeof r === "string" ? r : null;
  },
  async pickFile(): Promise<string | null> {
    if (!inTauri) return null;
    const { open } = await import("@tauri-apps/plugin-dialog");
    const r = await open({ directory: false, title: "Choose file" });
    return typeof r === "string" ? r : null;
  },
};

export type { FullState, Item, ItemOptions, Settings, ToolsState };
