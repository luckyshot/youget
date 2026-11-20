# Public release review

Review YouGet end to end against the behavior advertised in the README and the packages people will install. Keep fixes focused on correctness, trust, usability and maintainability. Preserve existing settings and queue data.

## 1. Establish a baseline

- [ ] Inventory the source, assets, build scripts, dependencies, workflows and packages. Record available platforms and tools.
- [ ] Run frontend type checks and build, Rust formatting, linting and tests. Record failures with reproduction steps.
- [ ] Check the existing release tags and artifacts before changing release guidance.

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
