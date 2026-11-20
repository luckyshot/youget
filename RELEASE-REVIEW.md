# Public release review

Review YouGet end to end against the behavior advertised in the README and the packages people will install. Keep fixes focused on correctness, trust, usability and maintainability. Preserve existing settings and queue data.

## 1. Establish a baseline

- [x] Inventory the source, assets, build scripts, dependencies, workflows and packages. Record available platforms and tools.
- [x] Run frontend type checks and build, Rust formatting, linting and tests. Record failures with reproduction steps.
- [x] Check the existing release tags and artifacts before changing release guidance.

## 2. Review the backend

- [ ] Trace startup, settings defaults and migration, queue persistence and recovery, and shutdown.
- [ ] Exercise the queue from URL and playlist intake through output. Cover parallelism, reordering, pause, resume, cancel, retry, removal and failures.
- [ ] Check yt-dlp command construction, format choices, metadata, progress and output parsing against representative fixtures. Check path handling and errors from child processes.
- [ ] Review tool bootstrapping and updates, including architecture selection, provenance, checksums, failed downloads, concurrent updates and recovery.
- [ ] Review Tauri permissions and command inputs, local process execution, remote content, and exposure of cookies, proxies and paths in logs and copied commands.

## 3. Review the frontend

- [ ] Check the frontend/backend contract and mock parity. Exercise cold start, setup failure, empty state, adding links, editing items, running and recovering downloads.
- [ ] Review keyboard access, focus, control labels, error feedback, contrast and window sizing.
- [ ] Check rendering and progress updates with a large queue. Fix measured bottlenecks or unnecessary work. Simplify duplicated logic and remove confirmed dead code.

## 4. Verify packages and CI

- [ ] Check reproducible dependency installs, version stamping, workflow permissions, package metadata and dependencies, desktop entries, icons and artifact names.
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
