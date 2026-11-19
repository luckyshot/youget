# YouGet

A desktop app for [yt-dlp](https://github.com/yt-dlp/yt-dlp) on Linux and macOS.
Drop a link, pick **Video** or **Audio**, and it downloads. No terminal needed.

- **Drag & drop or paste** (⌘V / Ctrl+V anywhere): single videos, several links at once, playlists and whole channels
- **Queue with live progress**: speed, time left per item, and an estimate for the whole queue
- **Parallel downloads** (1–8), pause / resume / cancel / retry, drag to reorder; the queue survives restarts
- **Video**: Best, 4K … 360p as MP4 (most compatible), MKV or WebM
- **Audio**: Opus (default, no re-encode), M4A, MP3, FLAC or WAV, with cover art and tags
- **Keeps yt-dlp up to date** automatically (stable or nightly channel)
- **Advanced** (hidden until you need it): filename templates with live preview, subtitles,
  SponsorBlock, browser cookies for age-restricted/members-only videos, rate limit, proxy,
  playlist ranges, download archive, clips (time ranges), exact format picker, extra yt-dlp arguments
- **Copy as command** on any item shows the exact yt-dlp command line

## Install

Download the latest package from [Releases](../../releases/latest).

| Platform | File | Install |
|---|---|---|
| Arch Linux | `youget-*.pkg.tar.zst` | `sudo pacman -U youget-*.pkg.tar.zst` |
| Debian 12+ / Ubuntu 24.04+ | `YouGet_*_amd64.deb` | `sudo apt install ./YouGet_*_amd64.deb` |
| macOS 11+ (Apple Silicon & Intel) | `YouGet_*_universal.dmg` | Open, drag to Applications |

**macOS:** the app is not notarized by Apple. On first launch either right-click the app → **Open**, or run:

```bash
xattr -dr com.apple.quarantine /Applications/YouGet.app
```

### What it installs by itself

On first launch YouGet sets up the tools it drives, inside its own data folder
(`~/.local/share/io.github.luckyshot.youget/bin` on Linux,
`~/Library/Application Support/io.github.luckyshot.youget/bin` on macOS):

- **yt-dlp**: the official standalone build, checksum-verified, self-updated daily.
- **ffmpeg**: uses your system copy if it works. Otherwise it is downloaded on Apple Silicon Macs.
  On Linux it is a package dependency. On Intel Macs: `brew install ffmpeg`.
- **Deno**: yt-dlp needs a JavaScript runtime for full YouTube support. Uses your system copy if
  present, otherwise downloads the official build.

## Development

Requirements: Rust (stable), Node.js 20.19+ and, on Linux, the WebKitGTK 4.1 development packages
(`webkit2gtk-4.1` on Arch, `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev` on Debian).

```bash
npm install
npm run tauri dev      # run the app with hot reload
npm run dev            # UI only, in a browser, with a mock backend and demo data
npm run check          # type-check the frontend
cd src-tauri && cargo test
```

Build a local package:

```bash
npm run tauri build -- --bundles deb      # Debian
npm run tauri build -- --bundles dmg      # macOS
cd packaging/arch && makepkg -si          # Arch
```

### Layout

- `src/`: Svelte 5 UI (`App.svelte`, queue rows, advanced drawer, per-item editor)
- `src-tauri/src/tools.rs`: finds, downloads, verifies and updates yt-dlp, ffmpeg and Deno
- `src-tauri/src/queue.rs`: queue, worker pool, yt-dlp argument building, progress parsing
- `src-tauri/src/settings.rs`: persisted settings
- `packaging/`: Arch PKGBUILD and Linux desktop entry
- `.github/workflows/release.yml`: builds Arch, Debian and macOS packages and publishes a release

## Releasing

```bash
git tag v0.2.0
git push origin v0.2.0
```

The tag sets the version in every package. The Release workflow builds all three platforms and
publishes them to GitHub Releases.

## License

MIT
