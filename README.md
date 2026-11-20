# YouGet

A desktop app for [yt-dlp](https://github.com/yt-dlp/yt-dlp) on Linux and macOS.
Drop a link, pick **Video** or **Audio**, and it downloads. Once installed, you can manage downloads from the app.

- **Drag & drop or paste** (⌘V / Ctrl+V anywhere): single videos, several links at once, playlists and whole channels
- **Queue with live progress**: speed, time left per item, and an estimate for the whole queue
- **Parallel downloads** (1–8), pause / resume / cancel / retry, drag or use **Move up** and **Move down** to reorder; the queue survives restarts
- **Video**: Best, 4K … 360p as MP4, MKV or WebM when the source offers WebM streams
- **Audio**: Opus (default), M4A, MP3, FLAC or WAV, with cover art and tags
- **Keeps yt-dlp up to date** automatically (stable or nightly channel)
- **Advanced** (hidden until you need it): filename templates with live preview, subtitles,
  SponsorBlock, browser cookies for age-restricted/members-only videos, rate limit, proxy,
  playlist ranges, download archive, clips (time ranges), exact format picker, extra yt-dlp arguments
- **Copy as command** on any item shows the exact yt-dlp command line

## Install

Download the latest package from [Releases](../../releases/latest).

| Platform | File | Install |
|---|---|---|
| Arch Linux (x86_64) | `youget-*.pkg.tar.zst` | `sudo pacman -U youget-*.pkg.tar.zst` |
| Debian 12+ / Ubuntu 24.04+ (amd64) | `YouGet_*_amd64.deb` | `sudo apt install ./YouGet_*_amd64.deb` |
| macOS 11+ (Apple Silicon & Intel) | `YouGet_*_universal.dmg` | Open, drag to Applications |

**macOS:** YouGet is ad hoc signed and has no Apple notarization. If macOS blocks the
app after you try to open it, follow Apple’s instructions for approving an app you trust.
[Apple Support](https://support.apple.com/en-us/102445)

### What it installs by itself

On first launch YouGet sets up the tools it drives, inside its own data folder
(`~/.local/share/io.github.luckyshot.youget/bin` on Linux,
`~/Library/Application Support/io.github.luckyshot.youget/bin` on macOS):

- **yt-dlp**: the official standalone build, checksum-verified. YouGet checks for updates on launch, at most once per day by default.
- **ffmpeg**: uses your system copy if it works. Otherwise it is downloaded on Apple Silicon Macs.
  On Linux it is a package dependency. On Intel Macs: `brew install ffmpeg`.
- **Deno**: yt-dlp needs a JavaScript runtime for full YouTube support. Uses your system copy if
  present, otherwise downloads the official build.

The default save folder is `Downloads/YouGet`. Change it in the main window.
If a tool fails to set up, check the status under **Advanced → yt-dlp & tools**, then use
**Re-check tools** after restoring the connection or installing the missing dependency.
If a download fails, open its log for the yt-dlp error. YouGet masks configured proxy and
cookie arguments in its logs. Logs can still include URLs and local paths. **Copy as command**
includes the exact options, which may contain credentials. Review both before sharing.
Exact video format selection keeps the source container for a single stream.
When merging separate video and audio streams, the chosen container must support both codecs.

## Development

Requirements: Rust (stable), Node.js 20.19+ and, on Linux, the WebKitGTK 4.1 development packages
(`webkit2gtk-4.1` on Arch, `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev` on Debian).

```bash
npm ci
npm run tauri dev      # run the app with hot reload
npm run dev            # UI only, in a browser, with a mock backend and demo data
npm run check          # type-check the frontend
(cd src-tauri && cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked)
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

Run the **Release** workflow manually on the candidate branch to build artifacts for
testing. Install the artifacts on each supported platform and verify the app launches,
sets up its tools, downloads a sample, and opens the output folder.

Push a new version tag after the checks pass.

```bash
git tag v0.2.0
git push origin v0.2.0
```

The tag sets the version in every package. The Release workflow builds all three platforms and
publishes them to GitHub Releases.

## License

MIT
