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
  /** Unix seconds, ticking once a second (for countdowns). */
  now: Math.floor(Date.now() / 1000),
});

setInterval(() => (app.now = Math.floor(Date.now() / 1000)), 1000);

let toastTimer: ReturnType<typeof setTimeout> | undefined;
export function toast(msg: string, ms = 3000) {
  app.toast = msg;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (app.toast = ""), ms);
}

let saveTimer: ReturnType<typeof setTimeout> | undefined;
let pendingSave = Promise.resolve();

export function flushSettings() {
  clearTimeout(saveTimer);
  if (!app.settings) return pendingSave;
  const settings = $state.snapshot(app.settings) as Settings;
  const save = pendingSave.then(() => api.saveSettings(settings));
  pendingSave = save.catch(() => {});
  return save;
}

/** Persist settings shortly after the last change. */
export function saveSettings() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    void flushSettings().catch((error) => toast(String(error), 6000));
  }, 250);
}

export async function init() {
  const buffered: (() => void)[] = [];
  let initialized = false;
  let capturing = false;
  function apply(update: () => void) {
    if (initialized) update();
    else if (capturing) buffered.push(update);
  }

  const results = await Promise.allSettled([
    on<Item>("item", (item) => apply(() => {
      const index = app.items.findIndex((entry) => entry.id === item.id);
      if (index >= 0) app.items[index] = item;
    })),
    on<Item[]>("items", (items) => apply(() => (app.items = items))),
    on<ToolsState>("tools", (tools) => apply(() => (app.tools = tools))),
  ]);
  const dispose = () => {
    for (const result of results) {
      if (result.status === "fulfilled") result.value();
    }
  };

  try {
    for (const result of results) {
      if (result.status === "rejected") throw result.reason;
    }
    capturing = true;
    const state = await api.getState();
    app.settings = state.settings;
    app.items = state.items;
    app.tools = state.tools;
    app.version = state.version;
    app.platform = state.platform;
    for (const update of buffered) update();
    buffered.length = 0;
    initialized = true;
    app.ready = true;
    return dispose;
  } catch (error) {
    dispose();
    throw error;
  }
}
