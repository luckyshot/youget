# Public release review

Review YouGet end to end against the behavior advertised in the README and the packages people will install. Keep fixes focused on correctness, trust, usability and maintainability. Preserve existing settings and queue data.

## 1. Establish a baseline

- [x] Inventory the source, assets, build scripts, dependencies, workflows and packages. Record available platforms and tools.
- [x] Run frontend type checks and build, Rust formatting, linting and tests. Record failures with reproduction steps.
- [x] Check the existing release tags and artifacts before changing release guidance.

## 2. Review the backend

- [x] Trace startup, settings defaults and migration, queue persistence and recovery, and shutdown.
- [ ] Exercise the queue from URL and playlist intake through output. Cover parallelism, reordering, pause, resume, cancel, retry, removal and failures.
- [x] Check yt-dlp command construction, format choices, metadata, progress and output parsing against representative fixtures. Check path handling and errors from child processes.
- [ ] Review tool bootstrapping and updates, including architecture selection, provenance, checksums, failed downloads, concurrent updates and recovery.
- [x] Review Tauri permissions and command inputs, local process execution, remote content, and exposure of cookies, proxies and paths in logs and copied commands.

## 3. Review the frontend

- [ ] Check the frontend/backend contract and mock parity. Exercise cold start, setup failure, empty state, adding links, editing items, running and recovering downloads.
- [x] Review keyboard access, focus, control labels, error feedback, contrast and window sizing.
- [x] Check rendering and progress updates with a large queue. Fix measured bottlenecks or unnecessary work. Simplify duplicated logic and remove confirmed dead code.

## 4. Verify packages and CI

- [x] Check reproducible dependency installs, version stamping, workflow permissions, package metadata and dependencies, desktop entries, icons and artifact names.
- [ ] Build and smoke-test the macOS, Arch and Debian packages on the applicable platforms. Verify startup, managed tools, a permitted sample download and discoverability of the output.
- [ ] Record exact platform and architecture coverage, CI results and any checks that require external access.

## 5. Document and deliver

- [ ] Align README feature claims with shipped behavior. Verify install instructions, supported platforms, first-run behavior, tool updates, troubleshooting and developer commands.
- [ ] Document the release procedure and known limitations. Check first-time user instructions against the built artifacts.
- [ ] Add focused regression coverage where the review exposes gaps. Rerun the relevant checks after each fix and the full available suite before finishing.
- [ ] Commit and push coherent verified changes. Record the verification results and concrete outstanding blockers. Leave release tags to a separate publishing decision.

## Verification notes

- Local host is macOS 15.3 (Apple Silicon), with Node 26.10.0 and the native Rust target. The Intel Rust target and Linux package build environments were absent at baseline.
- Baseline frontend type check and production build passed. All five original Rust tests passed. `cargo fmt --check` reported formatting differences, and `cargo clippy --locked --all-targets -- -D warnings` found two unnecessary string and iterator operations.
- Release build changes in `03c734b` passed a clean offline npm install after stamping `v0.2.0` in a temporary checkout. The Arch recipe passed `bash -n`. The Apple Silicon debug app built with `--locked`, passed `codesign --verify --deep --strict`, and declared `icon.icns` in its bundle metadata. Release DMG, Intel and Linux smoke tests are pending.
- The public release inventory showed [v0.1.0](https://github.com/luckyshot/youget/releases/tag/v0.1.0) with x86_64 Arch, amd64 Debian and universal macOS assets. The [v0.1.1 release run](https://github.com/luckyshot/youget/actions/runs/44677544858) completed successfully.
- The Apple Silicon debug app at `9c4931a` passed a native loopback smoke test with an isolated data directory, system yt-dlp, Deno, and a working local ffmpeg/ffprobe pair. The fixture verified duplicate suppression, a completed MP4 with audio and video, a valid saved output path, a failed URL, and persisted queue state. The host Homebrew ffmpeg was missing a dylib, so the test used the previously installed YouGet copy.
- A native Apple Silicon playlist fixture returned controlled metadata and downloaded two actual loopback MP4s through yt-dlp. It verified duplicate removal inside a playlist, retained numbering (1 and 3), group and output folder assignment, and persistence. A Rust regression test verifies that both current and legacy failed playlist entries retry metadata expansion.
- A headless Chrome preview verified the link input label, selected download mode, named item editor and log dialogs, Tab focus containment, Escape dismissal, and focus restoration to the originating queue action. Native WebKit and screen-reader checks remain open.
- Inspected Tauri's main-window capability and content security policy, native process arguments, settings and queue writes, and diagnostic output. Unit tests verify masking configured proxy and cookie arguments and replacing persisted files with owner-only permissions. Exact command copies intentionally retain their arguments.
- Fresh Apple Silicon setup downloaded the managed yt-dlp, FFmpeg, and FFprobe binaries into an isolated profile. The native queue completed a loopback MP4 and recorded the expected failed URL. The smoke harness timed out on an extra standalone yt-dlp version invocation after the download assertions passed. Deno used the system binary on this host, so its fresh install remains unverified. Upstream archive and checksum paths were checked against the publishers.
- All three [v0.1.1 release assets](https://github.com/luckyshot/youget/releases/tag/v0.1.1) matched their GitHub SHA-256 digests. The Arch x86_64 and Debian amd64 archives contained the executables, desktop launchers and hicolor icons. The macOS image verified and mounted read-only, with an Applications shortcut, bundle icon, ad hoc signature and both arm64 and x86_64 slices. This was package inspection for the existing tag. Current-source platform runtime checks remain open.
- Linux CI passed through [bd56363](https://github.com/luckyshot/youget/actions/runs/44680152971) when checked. The new release gates check Linux package contents and verify the macOS image, signature, icons and architecture slices before upload.
- Built the Apple Silicon release DMG from `37309e8`. The sandbox blocked temporary disk image creation, so the bundler was retried with native disk image access. The resulting 3.2 MiB DMG mounted read-only and passed signature, architecture, icon and Applications shortcut checks. The release workflow's lipo check now places the input path before the architecture flags.
- Progress parser fixtures cover separate streams, estimated and unknown totals, non-finite telemetry, an extractor ETA, and a partial filename containing a delimiter.
- Queue stop handling preserves pause and cancel requests during shutdown, prevents new child-process launches once shutdown starts, and removes only partial files inside the original destination. Rust tests cover stop precedence and preservation of completed and out-of-directory files.
- A 1,208-row headless Chrome preview with one active mock download recorded 224 layout passes and 1.64 seconds of renderer task time over four seconds before the queue update changes. After indexing rows, applying item updates in place, and removing continuous progress animation, the same fixture recorded 16 layout passes and 0.25 seconds of renderer task time. These are local fixture measurements.
- A native Apple Silicon fixture downloaded a WebM with both video and audio and rejected an MP4-only input in WebM mode. The exact audio stream survived save and reopen in the browser preview. The format argument test covers exact audio and strict WebM selection.
- A 720 × 520 headless Chrome test verified that the header fit at the minimum window size and that Enter could move an item within a playlist and move a standalone item past a playlist group. Focus remained on the item's More button.
- An isolated Apple Silicon app build displayed a controlled metadata item as Canceled after retry and cancellation. The child process had exited, queue.json recorded the canceled status, and no output was written. Regression fixtures also verify that older settings retain values and receive missing defaults, and that interrupted, paused and finished items recover as intended.
- Sampled primary button, muted text, accent text and warning/error combinations in the browser preview. The lowest measured ratios were 4.63:1 in light mode and 4.77:1 in dark mode. Verified a visible focus outline around the link entry form. This checks selected color pairs and does not constitute a full accessibility conformance audit.
