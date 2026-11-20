// In-browser stand-in for the Rust backend, used only when the UI is opened in a
// normal browser (`npm run dev`) for design work. Not bundled into app logic paths
// when running inside Tauri.
import type { FullState, Item, ItemOptions, Settings } from "./types";

type Cb = (p: unknown) => void;
const listeners = new Map<string, Set<Cb>>();
const emit = (ev: string, p: unknown) => listeners.get(ev)?.forEach((cb) => cb(p));

const defaults: ItemOptions = {
  mode: "video",
  video_quality: "1080",
  container: "mp4",
  audio_format: "opus",
  audio_quality: "best",
  format_id: null,
  clip_start: null,
  clip_end: null,
};

const settings: Settings = {
  defaults,
  output_dir: "~/Downloads/YouGet",
  parallel: 2,
  filename_template: "%(title)s.%(ext)s",
  playlist_subfolder: true,
  playlist_numbering: true,
  restrict_filenames: false,
  overwrite: "skip",
  embed_thumbnail: true,
  embed_metadata: true,
  embed_chapters: true,
  split_chapters: false,
  subtitles: false,
  sub_langs: "en",
  auto_subs: false,
  embed_subs: true,
  sub_format: "srt",
  sponsorblock: "off",
  sponsorblock_categories: ["sponsor", "selfpromo", "interaction"],
  cookies_browser: "",
  cookies_profile: "",
  cookies_file: "",
  rate_limit: "",
  concurrent_fragments: 4,
  retries: 10,
  proxy: "",
  playlist_items: "",
  playlist_reverse: false,
  playlist_random: false,
  use_archive: false,
  codec_pref: "any",
  force_keyframes: false,
  channel: "stable",
  auto_update: true,
  custom_ytdlp: "",
  notifications: true,
  extra_args: "",
  last_update_check: 0,
};

let n = 0;
function mk(p: Partial<Item>): Item {
  return {
    id: `m${n++}`,
    url: "https://www.youtube.com/watch?v=demo" + n,
    title: "Demo video",
    uploader: "Some Channel",
    thumbnail: null,
    duration: 245,
    group_id: null,
    group_title: null,
    group_index: null,
    status: "queued",
    stage: "",
    downloaded: 0,
    total: null,
    speed: null,
    eta: null,
    percent: 0,
    filepath: null,
    error: null,
    opts: { ...defaults },
    added_at: n,
    attempts: 0,
    retry_at: 0,
    ...p,
  };
}

const items: Item[] = [
  mk({ title: "Building a tiny synthesizer from scratch", uploader: "Circuit Corner", status: "downloading", stage: "Downloading video", downloaded: 82e6, total: 140e6, speed: 4.2e6, eta: 14, percent: 58 }),
  mk({ title: "Late-night lo-fi session, 2 hours", uploader: "Quiet Beats", duration: 7260, status: "processing", stage: "Converting audio", percent: 100, total: 96e6, downloaded: 96e6, opts: { ...defaults, mode: "audio" } }),
  mk({ title: "How compilers actually work", uploader: "Byte Size", status: "done", percent: 100, total: 212e6, downloaded: 212e6, filepath: "/tmp/x.mp4" }),
  mk({ title: "This video is private", status: "error", error: "This video is private." }),
  ...[1, 2, 3, 4].map((i) =>
    mk({ title: `Synth DIY part ${i}`, uploader: "Circuit Corner", group_id: "g1", group_title: "Synth DIY series", group_index: i, duration: 600 + i * 90, status: i === 1 ? "done" : "queued", percent: i === 1 ? 100 : 0 }),
  ),
];

const state: FullState = {
  settings,
  items,
  tools: {
    ytdlp: { path: "/demo/yt-dlp", version: "2026.11.10", status: "ok", message: null, managed: true },
    ffmpeg: { path: "/demo/ffmpeg", version: "8.0", status: "ok", message: null, managed: false },
    deno: { path: "/demo/deno", version: "2.5.0", status: "ok", message: null, managed: true },
  },
  version: "0.0.0-preview",
  platform: "macos",
};

setInterval(() => {
  for (const it of items) {
    if (it.status === "downloading" && it.total) {
      it.downloaded = Math.min(it.total, it.downloaded + (it.speed ?? 0) / 4);
      it.percent = (it.downloaded / it.total) * 100;
      it.eta = (it.total - it.downloaded) / (it.speed ?? 1);
      if (it.downloaded >= it.total) {
        it.downloaded = 0;
      }
      emit("item", { ...it });
    }
  }
}, 250);

export const mock = {
  async invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
    switch (cmd) {
      case "get_state":
        return structuredClone(state);
      case "add_urls": {
        const active = new Set(items.filter((item) =>
          !["done", "error", "canceled"].includes(item.status)).map((item) => item.url));
        let added = 0;
        for (const input of (args?.urls as string[]) ?? []) {
          const url = input.trim();
          if (!url || active.has(url)) continue;
          active.add(url);
          items.push(mk({ url, title: url, opts: { ...((args?.opts as ItemOptions) ?? settings.defaults) } }));
          added++;
        }
        if (added) emit("items", structuredClone(items));
        return added;
      }
      case "save_settings":
        Object.assign(settings, args?.settings);
        return;
      case "item_action": {
        const i = items.findIndex((x) => x.id === args?.id);
        if (i >= 0 && args?.action === "remove") items.splice(i, 1);
        else if (i >= 0) items[i].status = args?.action === "pause" ? "paused" : args?.action === "cancel" ? "canceled" : "queued";
        emit("items", structuredClone(items));
        return;
      }
      case "set_item_options":
      case "set_pending_options":
        for (const item of items) {
          if (!["queued", "paused", "fetching"].includes(item.status)) continue;
          const opts = args?.opts as ItemOptions;
          if (cmd === "set_pending_options") {
            const { format_id, clip_start, clip_end } = item.opts;
            item.opts = { ...opts, format_id, clip_start, clip_end };
          } else if (item.id === args?.id) {
            item.opts = { ...opts };
          }
        }
        emit("items", structuredClone(items));
        return;
      case "reorder": {
        const ids = args?.ids as string[];
        const byId = new Map(items.map((item) => [item.id, item]));
        const ordered = ids.flatMap((id) => {
          const item = byId.get(id);
          byId.delete(id);
          return item ? [item] : [];
        });
        items.splice(0, items.length, ...ordered, ...byId.values());
        emit("items", structuredClone(items));
        return;
      }
      case "clear_finished":
        for (let index = items.length - 1; index >= 0; index--) {
          if (["done", "canceled"].includes(items[index].status)) items.splice(index, 1);
        }
        emit("items", structuredClone(items));
        return;
      case "pause_all":
      case "resume_all":
        for (const item of items) {
          if (cmd === "pause_all" && ["queued", "downloading", "processing"].includes(item.status)) {
            item.status = "paused";
          } else if (cmd === "resume_all" && item.status === "paused") {
            item.status = "queued";
          }
        }
        emit("items", structuredClone(items));
        return;
      case "command_line":
        return "yt-dlp -f 'bv*+ba/b' -S res:1080,vcodec:h264,acodec:aac -- https://www.youtube.com/watch?v=demo";
      case "get_log":
        return ["[youtube] Extracting URL", "[info] Downloading 1 format(s): 137+140"];
      case "list_formats":
        return {
          title: "Demo",
          duration: 245,
          formats: [
            { id: "137", ext: "mp4", height: 1080, width: 1920, fps: 30, vcodec: "avc1.640028", acodec: "none", size: 120e6, tbr: 4000, note: "1080p" },
            { id: "248", ext: "webm", height: 1080, width: 1920, fps: 30, vcodec: "vp9", acodec: "none", size: 90e6, tbr: 3000, note: "1080p" },
            { id: "140", ext: "m4a", height: null, width: null, fps: null, vcodec: "none", acodec: "mp4a.40.2", abr: 129, size: 4e6, note: "medium" },
            { id: "251", ext: "webm", height: null, width: null, fps: null, vcodec: "none", acodec: "opus", abr: 135, size: 4.2e6, note: "medium" },
          ],
        };
      default:
        return null;
    }
  },
  listen(ev: string, cb: Cb): () => void {
    if (!listeners.has(ev)) listeners.set(ev, new Set());
    listeners.get(ev)!.add(cb);
    return () => listeners.get(ev)!.delete(cb);
  },
};
