export function fmtBytes(n: number | null | undefined): string {
  if (n == null || !isFinite(n) || n <= 0) return "";
  const u = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  while (n >= 1000 && i < u.length - 1) {
    n /= 1000;
    i++;
  }
  return `${n.toFixed(n >= 100 || i === 0 ? 0 : 1)} ${u[i]}`;
}

export function fmtSpeed(n: number | null | undefined): string {
  const b = fmtBytes(n);
  return b ? `${b}/s` : "";
}

export function fmtDuration(sec: number | null | undefined): string {
  if (sec == null || !isFinite(sec) || sec < 0) return "";
  sec = Math.round(sec);
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = sec % 60;
  const mm = h ? String(m).padStart(2, "0") : String(m);
  return `${h ? h + ":" : ""}${mm}:${String(s).padStart(2, "0")}`;
}

/** Friendly "time left": 45s, 3 min, 1 h 20 min */
export function fmtEta(sec: number | null | undefined): string {
  if (sec == null || !isFinite(sec) || sec < 0) return "";
  sec = Math.round(sec);
  if (sec < 60) return `${sec}s`;
  const m = Math.round(sec / 60);
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60);
  return `${h} h ${m % 60} min`;
}

const URL_RE = /https?:\/\/[^\s"'<>\]\[)(]+/gi;

/** Pull every http(s) URL out of arbitrary text (pastes, .txt, .webloc, .url files). */
export function extractUrls(text: string): string[] {
  const found = text.match(URL_RE) ?? [];
  const clean = found.map((u) => u.replace(/[.,;]+$/, "").replace(/&amp;/g, "&"));
  return [...new Set(clean)];
}

const SAMPLE: Record<string, string> = {
  title: "Never Gonna Give You Up",
  id: "dQw4w9WgXcQ",
  uploader: "Rick Astley",
  channel: "Rick Astley",
  upload_date: "20091025",
  ext: "mp4",
  playlist_title: "My Playlist",
  playlist_index: "1",
  resolution: "1920x1080",
  height: "1080",
  duration_string: "3:33",
  extractor: "youtube",
  release_year: "1987",
};

/** Approximate preview of a yt-dlp output template using sample metadata. */
export function previewTemplate(tmpl: string, ext: string): string {
  if (!tmpl.trim()) tmpl = "%(title)s.%(ext)s";
  if (!tmpl.includes("%(ext)s")) tmpl += ".%(ext)s";
  return tmpl.replace(/%\(([^)]+)\)([-0-9.]*)([a-zA-Z])/g, (_m, expr: string, width: string, conv: string) => {
    // Support "a,b" alternatives, ">date format" and "|default".
    let [fields, def] = expr.split("|");
    fields = fields.split(">")[0];
    for (const f of fields.split(",")) {
      const k = f.trim();
      if (k === "ext") return ext;
      if (k === "upload_date" && expr.includes(">")) return "2009-10-25";
      if (k in SAMPLE) {
        if (conv === "d" && width) return SAMPLE[k].padStart(parseInt(width, 10) || 0, "0");
        return SAMPLE[k];
      }
    }
    return def ?? "NA";
  });
}

export const TEMPLATE_PRESETS: { label: string; value: string }[] = [
  { label: "Title", value: "%(title)s.%(ext)s" },
  { label: "Channel – Title", value: "%(uploader)s - %(title)s.%(ext)s" },
  { label: "Date – Title", value: "%(upload_date>%Y-%m-%d)s - %(title)s.%(ext)s" },
  { label: "Title [id]", value: "%(title)s [%(id)s].%(ext)s" },
  { label: "Channel folder / Title", value: "%(uploader)s/%(title)s.%(ext)s" },
];

export const TEMPLATE_TOKENS = [
  "title",
  "id",
  "uploader",
  "upload_date",
  "duration_string",
  "resolution",
  "ext",
];

export const VIDEO_QUALITIES = [
  { v: "best", l: "Best" },
  { v: "2160", l: "2160p (4K)" },
  { v: "1440", l: "1440p" },
  { v: "1080", l: "1080p" },
  { v: "720", l: "720p" },
  { v: "480", l: "480p" },
  { v: "360", l: "360p" },
];
export const CONTAINERS = [
  { v: "mp4", l: "MP4" },
  { v: "mkv", l: "MKV" },
  { v: "webm", l: "WebM" },
];
export const AUDIO_FORMATS = [
  { v: "opus", l: "Opus" },
  { v: "m4a", l: "M4A" },
  { v: "mp3", l: "MP3" },
  { v: "flac", l: "FLAC" },
  { v: "wav", l: "WAV" },
];
export const AUDIO_QUALITIES = [
  { v: "best", l: "Best" },
  { v: "320", l: "320 kbps" },
  { v: "192", l: "192 kbps" },
  { v: "128", l: "128 kbps" },
];

export function optsLabel(o: { mode: string; video_quality: string; container: string; audio_format: string; audio_quality: string; format_id: string | null }): string {
  if (o.format_id) return `Format ${o.format_id}`;
  if (o.mode === "audio") {
    const q = o.audio_format === "flac" || o.audio_format === "wav" || o.audio_quality === "best" ? "" : ` ${o.audio_quality}k`;
    return `${o.audio_format.toUpperCase()}${q}`;
  }
  const q = o.video_quality === "best" ? "Best" : `${o.video_quality}p`;
  return `${q} ${o.container.toUpperCase()}`;
}
