import { api, on } from "./api";
import type { Item, Settings, ToolsState } from "./types";

export const app = $state({
  ready: false,
  settings: null as Settings | null,
  items: [] as Item[],
  tools: null as ToolsState | null,
  version: "",
  platform: "",
  toast: "" as string,
});

let toastTimer: ReturnType<typeof setTimeout> | undefined;
export function toast(msg: string, ms = 3000) {
  app.toast = msg;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (app.toast = ""), ms);
}

let saveTimer: ReturnType<typeof setTimeout> | undefined;
/** Persist settings shortly after the last change. */
export function saveSettings() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    if (app.settings) api.saveSettings($state.snapshot(app.settings) as Settings);
  }, 250);
}

export async function init() {
  const s = await api.getState();
  app.settings = s.settings;
  app.items = s.items;
  app.tools = s.tools;
  app.version = s.version;
  app.platform = s.platform;
  app.ready = true;
  await on<Item>("item", (it) => {
    const i = app.items.findIndex((x) => x.id === it.id);
    if (i >= 0) app.items[i] = it;
  });
  await on<Item[]>("items", (items) => (app.items = items));
  await on<ToolsState>("tools", (t) => (app.tools = t));
}
