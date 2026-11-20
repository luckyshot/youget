export interface ItemOptions {
  mode: "video" | "audio";
  video_quality: string;
  container: string;
  audio_format: string;
  audio_quality: string;
  format_id: string | null;
  clip_start: string | null;
  clip_end: string | null;
}

export type Status =
  | "fetching"
  | "queued"
  | "downloading"
  | "processing"
  | "paused"
  | "done"
  | "error"
  | "canceled";

export interface Item {
  id: string;
  url: string;
  title: string;
  uploader: string | null;
  thumbnail: string | null;
  duration: number | null;
  group_id: string | null;
  group_title: string | null;
  group_index: number | null;
  status: Status;
  stage: string;
  downloaded: number;
  total: number | null;
  speed: number | null;
  eta: number | null;
  percent: number;
  filepath: string | null;
  error: string | null;
  opts: ItemOptions;
  added_at: number;
  attempts: number;
  retry_at: number;
  /** Backend bookkeeping; the UI only displays queue state. */
  retry_playlist?: boolean;
  /** Files written by unfinished attempts; removed on cancel or remove. */
  partials?: string[];
  work_dir?: string;
}

export interface Settings {
  defaults: ItemOptions;
  output_dir: string;
  parallel: number;
  filename_template: string;
  playlist_subfolder: boolean;
  playlist_numbering: boolean;
  restrict_filenames: boolean;
  overwrite: string;
  embed_thumbnail: boolean;
  embed_metadata: boolean;
  embed_chapters: boolean;
  split_chapters: boolean;
  subtitles: boolean;
  sub_langs: string;
  auto_subs: boolean;
  embed_subs: boolean;
  sub_format: string;
  sponsorblock: string;
  sponsorblock_categories: string[];
  cookies_browser: string;
  cookies_profile: string;
  cookies_file: string;
  rate_limit: string;
  concurrent_fragments: number;
  retries: number;
  proxy: string;
  playlist_items: string;
  playlist_reverse: boolean;
  playlist_random: boolean;
  use_archive: boolean;
  codec_pref: string;
  force_keyframes: boolean;
  channel: string;
  auto_update: boolean;
  custom_ytdlp: string;
  notifications: boolean;
  extra_args: string;
  last_update_check: number;
}

export interface ToolInfo {
  path: string | null;
  version: string | null;
  status: "checking" | "downloading" | "updating" | "ok" | "missing" | "error";
  message: string | null;
  managed: boolean;
}

export interface ToolsState {
  ytdlp: ToolInfo;
  ffmpeg: ToolInfo;
  deno: ToolInfo;
}

export interface FullState {
  settings: Settings;
  items: Item[];
  tools: ToolsState;
  version: string;
  platform: string;
}

export interface Format {
  id: string;
  ext: string;
  height: number | null;
  width: number | null;
  fps: number | null;
  vcodec: string | null;
  acodec: string | null;
  abr: number | null;
  tbr: number | null;
  size: number | null;
  note: string | null;
  hdr: string | null;
  lang: string | null;
}
