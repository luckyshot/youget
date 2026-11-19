# YouGet — Plan

A desktop GUI for yt-dlp on macOS and Linux. Drop a link in, pick Video or Audio, and get the file.
It's built for technical users who don't want to open a terminal.

Status: **v0.1 implemented**. This file records the original design and the decisions behind it.

---

## 1. Stack (proposed)

| Piece | Choice | Why |
|---|---|---|
| App shell | **Tauri 2** (Rust backend, HTML/TS frontend) | Native window, ~10 MB bundle, real drag-and-drop, one codebase for macOS and Linux. Rust and Node are already on this Mac. |
| UI | Svelte + TypeScript (Vite) | Small and fast, so the queue can update many progress bars without lag. |
| yt-dlp | **Managed standalone binary** in the app data dir (`yt-dlp_macos` / `yt-dlp_linux` from GitHub releases) | Doesn't depend on brew or pip. The app can update it without asking for permissions. |
| ffmpeg / ffprobe | Use the system copy if found. Otherwise download a static build into the app data dir. | Needed to merge video and audio, extract audio, and embed thumbnails and subtitles. |
| JS runtime | **Deno**, managed the same way | yt-dlp now needs an external JS runtime to handle full YouTube support (EJS challenge solving). Without it, many formats go missing. |
| Packaging | macOS `.dmg` (arm64 + x64), Linux AppImage and `.deb` | Tauri bundler. |

Alternative if you want "truly native" widgets: PySide6/Qt. It's heavier (~80 MB), with uglier packaging and slower UI iteration. I recommend Tauri.

### How we drive yt-dlp
- One `yt-dlp` child process per download.
- `--newline --progress-template` emits JSON lines. We parse them for %, speed, ETA, downloaded and total bytes, and the current stage (downloading, merging, post-processing).
- `--print after_move:filepath` returns the final path, used for "Show in Finder".
- Metadata and playlist expansion: `yt-dlp -J --flat-playlist <url>`. It's fast and gives us the title, thumbnail, duration and entry list before anything downloads.
- Cancel sends SIGINT, which lets yt-dlp clean up `.part` files. Pause is cancel plus resume, because yt-dlp resumes `.part` files natively.

### Keeping yt-dlp up to date
- On launch (at most once every 24 h), check the GitHub releases API for the latest tag and compare it with `yt-dlp --version`.
- If a newer version exists, download it into the app data dir, verify it against `SHA2-256SUMS`, and atomically swap it in. Never swap while downloads are running.
- Settings shows the version, a "Check now" button, and the update channel: **stable** (default) or **nightly**. Nightly is useful when YouTube breaks something.
- Deno and ffmpeg are checked the same way, but less often.

---

## 2. Main window: always visible

```
┌──────────────────────────────────────────────────────────────┐
│  ⬇  Drop or paste links here (⌘V anywhere)                    │
│                                                               │
│  [ Video | Audio ]   Quality [1080p ▾]   Format [MP4 ▾]       │
│  Save to  ~/Downloads/YouGet  [Change…]    Parallel [3 ▾]     │
│                                            ▸ Advanced         │
├──────────────────────────────────────────────────────────────┤
│ [thumb] Video title…            ███████░░░  72%  4.1 MB/s  0:38│
│ [thumb] Another title           queued                         │
│ [thumb] Playlist: Foo (24)  ▸   3/24 done             ~6:10    │
├──────────────────────────────────────────────────────────────┤
│ 3 active · 21 queued · 1.2 GB left · ~6 min      [Pause all]  │
└──────────────────────────────────────────────────────────────┘
```

**Input**
- Drag and drop URLs: from the browser address bar, links, text, and `.webloc`, `.url` or `.txt` files.
- Paste anywhere with ⌘V or Ctrl+V. Several URLs in one paste are split by whitespace or newlines.
- A playlist or channel URL expands into a group. You can download all of it, or open it and untick entries.
- Duplicate URLs already in the queue are ignored.

**Format (one row)**
- **Video / Audio** toggle.
- Video quality: Best · 2160p · 1440p · 1080p · 720p · 480p · 360p.
- Video container: MP4 (most compatible; prefers H.264 and AAC) · MKV · WebM.
- Audio format: MP3 · M4A · Opus · FLAC · WAV.
- Audio quality: Best · 320k · 192k · 128k (lossy formats only).
- These settings are the defaults for new items. Each queued item can also be changed before it starts, via a right-click menu or an item popover.

**Destination**
- Output folder picker, remembered between runs.
- "Open folder" button.

**Queue**
- Each item shows its thumbnail, title, channel, duration, a progress bar, speed, ETA and size.
- The current stage is labelled: *Fetching info → Downloading video → Downloading audio → Merging → Done*.
- Item actions: pause/resume, cancel, retry, remove, show in Finder/Files, open, copy URL.
- Drag to reorder. "Clear finished".
- **Parallel downloads**: 1–8, default 3. Changes apply live.
- Footer: active and queued counts, bytes left, and the **total ETA for the queue**. Pause all and resume all.
- The queue persists across restarts. Unfinished items resume.

**Feedback**
- A system notification when an item fails, and when the whole queue finishes.
- Errors are readable ("Video is private", "Sign-in required → enable browser cookies in Advanced"). The full log is one click away.
- Dark and light mode follow the OS.

---

## 3. Advanced: collapsed by default

Grouped into sections in a slide-out panel or a settings window.

**File naming**
- Filename template (`-o`) with presets (`Title`, `Channel - Title`, `Date - Title`, `Title [id]`), a token picker and a **live preview**.
- Playlist subfolder template (on/off; default `%(playlist_title)s/%(playlist_index)03d - %(title)s`).
- Restrict filenames to ASCII (`--restrict-filenames`).
- Overwrite policy: skip, overwrite, or rename.

**Embedding and metadata**
- Embed thumbnail (`--embed-thumbnail`). On by default for audio.
- Embed metadata (`--embed-metadata`). On by default.
- Embed chapters (`--embed-chapters`).
- Split by chapters (`--split-chapters`), useful for albums and mixes.

**Subtitles**
- Download subtitles: off, or a list of languages (`--sub-langs`, e.g. `en,es`).
- Include auto-generated (`--write-auto-subs`).
- Embed or save as separate file, and the format (srt or vtt).

**SponsorBlock**
- Off, mark as chapters (`--sponsorblock-mark`), or remove (`--sponsorblock-remove`).
- Category checkboxes: sponsor, intro, outro, selfpromo, interaction, music_offtopic, preview, filler.

**Access**
- Cookies from browser (`--cookies-from-browser`): None, Chrome, Firefox, Safari, Brave, Edge, Chromium, plus a profile name. Needed for age-restricted, members-only and private videos.
- Cookies file (`--cookies`) as an alternative.

**Network and performance**
- Rate limit (`-r`), e.g. `5M`.
- Concurrent fragments per download (`-N`), default 4.
- Retries (`--retries`, `--fragment-retries`).
- Proxy (`--proxy`).

**Playlists**
- Items range (`-I`, e.g. `1:10`, `-5:`).
- Reverse order and random order.
- Download archive (`--download-archive`): skip anything already downloaded once. Handy for channels you re-sync.

**Clip**
- Per item only: download a time range (`--download-sections "*1:30-3:45"`), with an option to force exact keyframe cuts.

**Formats, power user**
- A format picker for each item: lists all formats from `-F` (resolution, codec, fps, bitrate, size) so you can choose exact video and audio streams.
- Codec preference: H.264 / VP9 / AV1, or any.
- Prefer 60 fps / HDR.

**App**
- yt-dlp channel (stable or nightly), version, and "Update now".
- Custom yt-dlp binary path, to use your own instead of the managed one.
- Watch clipboard: offer to add copied YouTube links. Off by default.
- Notifications on or off.
- **Extra arguments** text field, appended to every command. An escape hatch for anything not covered here.
- "Copy as command" on any item, which shows the exact yt-dlp command line. It builds trust and helps debugging.

---

## 4. Not bothering with (v1)

- Username, password, 2FA, `.netrc` and video passwords (cookies cover real-world needs).
- `--exec`, custom post-processor chains, `--parse-metadata` and `--replace-in-metadata`.
- External downloaders (aria2c etc.), since `-N` is enough.
- Raw `-S` format-sort syntax and `-f` expressions in the UI (possible via the Extra arguments field).
- Live streams from start (`--live-from-start`) and waiting for scheduled premieres.
- Geo-bypass options, impersonation and `--xattrs`.
- Writing `.info.json`, descriptions, comments or annotations.
- Multiple output templates per file type.
- Download history browser or a library view (the queue plus "Clear finished" is enough).
- A dedicated UI for other sites. yt-dlp will still accept them; the UI is just tuned for YouTube.
- Windows (could come later; Tauri makes it cheap).

---

## 5. Milestones

1. **Skeleton**: Tauri app, binary manager (download, verify and update yt-dlp, Deno and ffmpeg), settings store.
2. **Core loop**: drop or paste a URL, fetch info, a single download with live progress, Video/Audio and quality and format, output folder.
3. **Queue**: multiple items, playlists expanded into groups, parallel worker pool, pause/cancel/retry, total ETA, persistence.
4. **Advanced panel**: section 3 options, filename preview, "Copy as command".
5. **Polish and packaging**: notifications, error mapping, icons, `.dmg` and AppImage/`.deb`, a build script per OS.

## 6. Decisions

- Stack: Tauri 2 + Svelte 5 + TypeScript.
- No Apple Developer ID: macOS builds are ad-hoc signed, not notarized.
- Linux targets: Arch (`.pkg.tar.zst`) and Debian (`.deb`).
- Audio default: Opus.
- Public GitHub repo; releases are built by GitHub Actions on `v*` tags.
- Clipboard watching was dropped from v1 (⌘V anywhere covers it).
- Overwrite policy is skip or overwrite (yt-dlp has no native "rename").
