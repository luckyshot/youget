<script lang="ts">
  import { onMount, tick } from "svelte";
  import Advanced from "./lib/Advanced.svelte";
  import Icon from "./lib/Icon.svelte";
  import ItemEditor from "./lib/ItemEditor.svelte";
  import QueueItem from "./lib/QueueItem.svelte";
  import { api } from "./lib/api";
  import { app, init, saveSettings, toast } from "./lib/store.svelte";
  import type { Item, ItemOptions } from "./lib/types";
  import {
    AUDIO_FORMATS,
    AUDIO_QUALITIES,
    CONTAINERS,
    dismissOnBackdrop,
    VIDEO_QUALITIES,
    extractUrls,
    fmtBytes,
    fmtEta,
    fmtSpeed,
  } from "./lib/util";

  let showAdvanced = $state(false);
  let editing = $state<Item | null>(null);
  let logDialog: HTMLDialogElement | undefined = $state();
  let logView = $state<{ title: string; text: string } | null>(null);
  let dragging = $state(false);
  let dragDepth = 0;
  let urlInput = $state("");
  let collapsed = $state<Record<string, boolean>>({});

  let startupError = $state("");
  let dispose: (() => void) | undefined;
  let unmounted = false;

  async function start() {
    app.ready = false;
    startupError = "";
    try {
      const stop = await init();
      if (unmounted) stop();
      else dispose = stop;
    } catch (error) {
      startupError = String(error);
    }
  }

  onMount(() => {
    void start();
    return () => {
      unmounted = true;
      dispose?.();
    };
  });

  const s = $derived(app.settings);
  const d = $derived(app.settings?.defaults);

  // ---------------------------------------------------------------- adding

  function add(urls: string[]) {
    if (!urls.length || !d) return;
    api.addUrls(urls, $state.snapshot(d) as ItemOptions);
    toast(urls.length === 1 ? "Added 1 link" : `Added ${urls.length} links`);
  }

  function submitInput() {
    const urls = extractUrls(urlInput);
    if (!urls.length) {
      toast("That doesn't look like a link");
      return;
    }
    urlInput = "";
    add(urls);
  }

  function onPaste(e: ClipboardEvent) {
    const t = e.target as HTMLElement;
    if (t.closest("input, textarea, [contenteditable]")) return;
    const text = e.clipboardData?.getData("text") ?? "";
    const urls = extractUrls(text);
    if (urls.length) {
      e.preventDefault();
      add(urls);
    } else {
      toast("No links found in the clipboard");
    }
  }

  const isItemDrag = (e: DragEvent) => !!e.dataTransfer?.types.includes("application/x-youget-item");

  function onDragEnter(e: DragEvent) {
    if (isItemDrag(e)) return;
    e.preventDefault();
    dragDepth++;
    dragging = true;
  }
  function onDragLeave(e: DragEvent) {
    if (isItemDrag(e)) return;
    dragDepth = Math.max(0, dragDepth - 1);
    if (dragDepth === 0) dragging = false;
  }
  function onDragOver(e: DragEvent) {
    e.preventDefault();
    if (e.dataTransfer && !isItemDrag(e)) e.dataTransfer.dropEffect = "copy";
  }
  async function onDrop(e: DragEvent) {
    e.preventDefault();
    dragDepth = 0;
    dragging = false;
    if (!e.dataTransfer || isItemDrag(e)) return;
    const dt = e.dataTransfer;
    let text = [dt.getData("text/uri-list"), dt.getData("text/plain"), dt.getData("text/html")].join("\n");
    for (const f of Array.from(dt.files)) {
      if (f.size < 2_000_000) {
        try {
          text += "\n" + (await f.text());
        } catch {
          /* unreadable file */
        }
      }
    }
    const urls = extractUrls(text);
    if (urls.length) add(urls);
    else toast("No links found in what you dropped");
  }

  // ---------------------------------------------------------------- reordering

  let dragId: string | null = null;
  function itemDragStart(e: DragEvent, id: string) {
    dragId = id;
    e.dataTransfer?.setData("application/x-youget-item", id);
    if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
  }
  function itemDrop(e: DragEvent, targetId: string) {
    if (!isItemDrag(e) || !dragId || dragId === targetId) return;
    e.preventDefault();
    e.stopPropagation();
    const ids = app.items.map((i) => i.id).filter((i) => i !== dragId);
    const at = ids.indexOf(targetId);
    ids.splice(at, 0, dragId);
    dragId = null;
    api.reorder(ids);
  }

  // ---------------------------------------------------------------- options

  function setOpt<K extends keyof ItemOptions>(k: K, v: ItemOptions[K]) {
    if (!s) return;
    s.defaults[k] = v;
    saveSettings();
  }

  const pendingCount = $derived(app.items.filter((i) => i.status === "queued" || i.status === "paused").length);
  const pendingDiffers = $derived(
    !!d &&
      app.items.some(
        (i) =>
          (i.status === "queued" || i.status === "paused") &&
          !i.opts.format_id &&
          (i.opts.mode !== d.mode ||
            (d.mode === "video" ? i.opts.video_quality !== d.video_quality || i.opts.container !== d.container : i.opts.audio_format !== d.audio_format || i.opts.audio_quality !== d.audio_quality)),
      ),
  );

  function applyToPending() {
    if (d) api.setPendingOptions($state.snapshot(d) as ItemOptions);
  }

  async function pickFolder() {
    if (!s) return;
    const dir = await api.pickFolder(s.output_dir);
    if (dir) {
      s.output_dir = dir;
      saveSettings();
    }
  }

  function setParallel(n: number) {
    if (!s) return;
    s.parallel = Math.max(1, Math.min(8, n));
    saveSettings();
  }

  // ---------------------------------------------------------------- grouping

  type Row = { kind: "item"; item: Item } | { kind: "group"; id: string; title: string; items: Item[] };
  const rows = $derived.by(() => {
    const out: Row[] = [];
    const groups = new Map<string, Row & { kind: "group" }>();
    for (const it of app.items) {
      if (it.group_id) {
        let g = groups.get(it.group_id);
        if (!g) {
          g = { kind: "group", id: it.group_id, title: it.group_title ?? "Playlist", items: [] };
          groups.set(it.group_id, g);
          out.push(g);
        }
        g.items.push(it);
      } else {
        out.push({ kind: "item", item: it });
      }
    }
    return out;
  });

  function groupStats(items: Item[]) {
    let done = 0;
    let failed = 0;
    let active = 0;
    let progress = 0;
    for (const item of items) {
      if (item.status === "done") done++;
      if (item.status === "error") failed++;
      if (item.status === "downloading" || item.status === "processing") active++;
      progress += item.status === "done" ? 100 : item.percent;
    }
    return { done, failed, active, pct: progress / Math.max(1, items.length) };
  }

  function groupAction(items: Item[], action: "pause" | "resume" | "remove") {
    for (const it of items) {
      if (action === "pause" && (it.status === "queued" || it.status === "downloading")) api.itemAction(it.id, "pause");
      if (action === "resume" && it.status === "paused") api.itemAction(it.id, "resume");
      if (action === "remove") api.itemAction(it.id, "remove");
    }
  }

  // ---------------------------------------------------------------- totals

  const totals = $derived.by(() => {
    let active = 0;
    let queued = 0;
    let paused = 0;
    let finished = 0;
    let speed = 0;
    let left = 0;
    let estimated = true;
    let knownBytes = 0;
    let knownDuration = 0;
    let waitingVideoDuration = 0;

    for (const item of app.items) {
      const { status, total, duration } = item;
      if (status === "downloading" || status === "processing") {
        active++;
        if (status === "downloading") {
          speed += item.speed ?? 0;
          if (total === null) estimated = false;
          else left += Math.max(0, total - item.downloaded);
        }
      } else if (status === "queued" || status === "fetching") {
        queued++;
        if (!duration) estimated = false;
        else if (item.opts.mode === "audio") left += duration * 16_000; // ~128 kbps
        else waitingVideoDuration += duration;
      } else if (status === "paused") {
        paused++;
      } else if (status === "done" || status === "canceled") {
        finished++;
      }
      // Learn video bytes per second from downloads whose size is known.
      if (total && duration && (status === "done" || status === "downloading") && item.opts.mode === "video") {
        knownBytes += total;
        knownDuration += duration;
      }
    }

    if (waitingVideoDuration > 0) {
      if (knownDuration > 0) left += waitingVideoDuration * knownBytes / knownDuration;
      else estimated = false;
    }
    const eta = speed > 0 && left > 0 ? left / speed : null;
    return { active, queued, paused, finished, speed, left, eta, estimated };
  });

  // ---------------------------------------------------------------- tools banner

  const banner = $derived.by(() => {
    const t = app.tools;
    if (!t) return null;
    if (t.ytdlp.status === "downloading" || t.ytdlp.status === "checking")
      return { kind: "info", text: t.ytdlp.message ?? "Setting up yt-dlp…" };
    if (t.ytdlp.status === "updating") return { kind: "info", text: t.ytdlp.message ?? "Updating yt-dlp…" };
    if (t.ytdlp.status === "error") return { kind: "err", text: t.ytdlp.message ?? "yt-dlp is not available" };
    if (t.ffmpeg.status === "missing" || t.ffmpeg.status === "error") return { kind: "warn", text: t.ffmpeg.message ?? "ffmpeg is missing" };
    if (t.ffmpeg.status === "downloading") return { kind: "info", text: t.ffmpeg.message ?? "Downloading ffmpeg…" };
    if (t.deno.status === "downloading") return { kind: "info", text: t.deno.message ?? "Downloading Deno…" };
    return null;
  });

  // ---------------------------------------------------------------- log/cmd

  async function showLog(it: Item, kind: "log" | "cmd") {
    try {
      if (kind === "cmd") {
        const command = await api.commandLine(it.id);
        logView = { title: "Command line", text: command ?? "" };
      } else {
        const lines = await api.getLog(it.id);
        logView = { title: `Log for ${it.title}`, text: lines.length ? lines.join("\n") : "No output yet." };
      }
      await tick();
      logDialog?.showModal();
    } catch (error) {
      toast(String(error));
    }
  }

  async function copyLog() {
    if (!logView) return;
    try {
      await navigator.clipboard.writeText(logView.text);
      toast("Copied");
    } catch {
      toast("Could not copy");
    }
  }

  const mod = $derived(app.platform === "macos" ? "⌘" : "Ctrl+");
  const shortDir = $derived(s?.output_dir.replace(/^\/(Users|home)\/[^/]+/, "~") ?? "");
</script>

<svelte:window
  onpaste={onPaste}
  ondragenter={onDragEnter}
  ondragleave={onDragLeave}
  ondragover={onDragOver}
  ondrop={onDrop}
  onkeydown={(e) => {
    if (e.key === "Escape" && !document.querySelector("dialog[open]")) {
      showAdvanced = false;
    }
  }}
/>

{#if app.ready && s && d}
  <div class="app">
    <header class="top">
      <div class="drop" class:big={app.items.length === 0}>
        {#if app.items.length === 0}
          <div class="hero">
            <div class="logo"><Icon name="down" size={30} /></div>
            <h1>Drop links here</h1>
            <p class="muted">Videos, playlists or channels. Or paste with <kbd>{mod}V</kbd> anywhere.</p>
          </div>
        {/if}
        <form class="add" onsubmit={(e) => { e.preventDefault(); submitInput(); }}>
          <Icon name="link" />
          <input type="text" aria-label="Links to download" placeholder="Paste a link (or several) and press Enter" bind:value={urlInput} spellcheck="false" autocomplete="off" />
          <button class="primary" type="submit" disabled={!urlInput.trim()}>Add</button>
        </form>
      </div>

      <div class="bar">
        <div class="seg">
          <button class:on={d.mode === "video"} aria-pressed={d.mode === "video"} onclick={() => setOpt("mode", "video")}><Icon name="film" size={14} /> Video</button>
          <button class:on={d.mode === "audio"} aria-pressed={d.mode === "audio"} onclick={() => setOpt("mode", "audio")}><Icon name="music" size={14} /> Audio</button>
        </div>
        {#if d.mode === "video"}
          <select value={d.video_quality} onchange={(e) => setOpt("video_quality", e.currentTarget.value)} title="Maximum resolution">
            {#each VIDEO_QUALITIES as q}<option value={q.v}>{q.l}</option>{/each}
          </select>
          <select value={d.container} onchange={(e) => setOpt("container", e.currentTarget.value)} title="Container">
            {#each CONTAINERS as q}<option value={q.v}>{q.l}</option>{/each}
          </select>
        {:else}
          <select value={d.audio_format} onchange={(e) => setOpt("audio_format", e.currentTarget.value)} title="Audio format">
            {#each AUDIO_FORMATS as q}<option value={q.v}>{q.l}</option>{/each}
          </select>
          {#if d.audio_format !== "flac" && d.audio_format !== "wav"}
            <select value={d.audio_quality} onchange={(e) => setOpt("audio_quality", e.currentTarget.value)} title="Audio quality">
              {#each AUDIO_QUALITIES as q}<option value={q.v}>{q.l}</option>{/each}
            </select>
          {/if}
        {/if}

        <div class="sep"></div>

        <button class="folder" onclick={pickFolder} title={s.output_dir}>
          <Icon name="folder" size={14} /><span>{shortDir}</span>
        </button>
        <button class="ghost" title="Open download folder" onclick={() => api.openDir(s.output_dir)}><Icon name="open" size={14} /></button>

        <div class="sep"></div>

        <div class="par" title="Parallel downloads">
          <span class="muted">Parallel</span>
          <button class="ghost step" aria-label="Decrease parallel downloads" onclick={() => setParallel(s.parallel - 1)} disabled={s.parallel <= 1}>−</button>
          <b>{s.parallel}</b>
          <button class="ghost step" aria-label="Increase parallel downloads" onclick={() => setParallel(s.parallel + 1)} disabled={s.parallel >= 8}>+</button>
        </div>

        <button class="adv" onclick={() => (showAdvanced = true)}><Icon name="gear" size={14} /> Advanced</button>
      </div>

      {#if pendingDiffers && pendingCount > 0}
        <div class="apply">
          New downloads will use these settings.
          <button class="link" onclick={applyToPending}>Apply to {pendingCount} waiting {pendingCount === 1 ? "item" : "items"}</button>
        </div>
      {/if}
      {#if banner}
        <div class="banner {banner.kind}">
          <Icon name={banner.kind === "info" ? "down" : "alert"} size={14} />
          <span>{banner.text}</span>
          {#if banner.kind !== "info"}<button class="link" onclick={() => (showAdvanced = true)}>Details</button>{/if}
        </div>
      {/if}
    </header>

    <main class="queue" role="list">
      {#each rows as row (row.kind === "item" ? row.item.id : row.id)}
        {#if row.kind === "item"}
          <QueueItem item={row.item} onedit={(it) => (editing = it)} onlog={showLog} ondragstart={itemDragStart} ondrop={itemDrop} />
        {:else}
          {@const g = groupStats(row.items)}
          <section class="group" role="listitem">
            <div class="ghead">
              <button class="ghost tog" class:open={!collapsed[row.id]} aria-label={`Toggle playlist ${row.title}`} aria-expanded={!collapsed[row.id]} onclick={() => (collapsed[row.id] = !collapsed[row.id])}><Icon name="chevron" size={14} /></button>
              <div class="gtitle">
                <b>{row.title}</b>
                <span class="muted">{g.done}/{row.items.length} done{g.failed ? ` · ${g.failed} failed` : ""}{g.active ? ` · ${g.active} active` : ""}</span>
              </div>
              <div class="gbar"><div style:transform="scaleX({g.pct / 100})"></div></div>
              <button class="ghost" title="Pause all in playlist" onclick={() => groupAction(row.items, "pause")}><Icon name="pause" size={14} /></button>
              <button class="ghost" title="Resume all in playlist" onclick={() => groupAction(row.items, "resume")}><Icon name="play" size={14} /></button>
              <button class="ghost" title="Remove playlist" onclick={() => groupAction(row.items, "remove")}><Icon name="x" size={14} /></button>
            </div>
            {#if !collapsed[row.id]}
              <div class="gitems" role="list" aria-label={row.title}>
                {#each row.items as it (it.id)}
                  <QueueItem item={it} nested onedit={(x) => (editing = x)} onlog={showLog} ondragstart={itemDragStart} ondrop={itemDrop} />
                {/each}
              </div>
            {/if}
          </section>
        {/if}
      {/each}
    </main>

    {#if app.items.length > 0}
      <footer>
        <div class="stats">
          {#if totals.active}<span><b>{totals.active}</b> downloading</span>{/if}
          {#if totals.queued}<span><b>{totals.queued}</b> waiting</span>{/if}
          {#if totals.paused}<span><b>{totals.paused}</b> paused</span>{/if}
          {#if totals.speed > 0}<span>{fmtSpeed(totals.speed)}</span>{/if}
          {#if totals.left > 0 && totals.active}<span>{totals.estimated ? "" : "≥ "}{fmtBytes(totals.left)} left</span>{/if}
          {#if totals.eta != null && totals.active}<span class="eta">{totals.estimated ? "About" : "At least"} {fmtEta(totals.eta)} to go</span>{/if}
          {#if !totals.active && !totals.queued && !totals.paused}<span class="muted">All done</span>{/if}
        </div>
        <div class="factions">
          {#if totals.active || totals.queued}
            <button onclick={() => api.pauseAll()}><Icon name="pause" size={13} /> Pause all</button>
          {/if}
          {#if totals.paused}
            <button onclick={() => api.resumeAll()}><Icon name="play" size={13} /> Resume all</button>
          {/if}
          <button onclick={() => api.clearFinished()} disabled={!totals.finished}>Clear finished</button>
        </div>
      </footer>
    {/if}
  </div>

  {#if dragging}
    <div class="dropover">
      <div><Icon name="down" size={40} /><p>Drop to add to the queue</p></div>
    </div>
  {/if}

  {#if showAdvanced}<Advanced onclose={() => (showAdvanced = false)} />{/if}
  {#if editing}<ItemEditor item={editing} onclose={() => (editing = null)} />{/if}
  {#if logView}
    <dialog class="logbox" bind:this={logDialog} aria-labelledby="log-title" onpointerdown={dismissOnBackdrop} onclose={() => (logView = null)}>
      <header><b id="log-title">{logView.title}</b><span><button onclick={copyLog}>Copy</button> <button class="ghost" aria-label="Close" onclick={() => logDialog?.close()}><Icon name="x" /></button></span></header>
      <pre class="selectable">{logView.text}</pre>
    </dialog>
  {/if}
{:else}
  <main class="startup">
    {#if startupError}
      <p role="alert">Could not start YouGet. {startupError}</p>
      <button onclick={start}>Retry</button>
    {:else}
      <p role="status">Starting YouGet…</p>
    {/if}
  </main>
{/if}

<div aria-live="polite">{#if app.toast}<div class="toast">{app.toast}</div>{/if}</div>

<style>
  .startup {
    display: grid;
    min-height: 100vh;
    place-content: center;
    justify-items: center;
    padding: 1rem;
  }
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  .top {
    padding: 14px 16px 10px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    border-bottom: 1px solid var(--border);
    background: var(--bg);
  }
  .drop.big {
    border: 2px dashed var(--border);
    border-radius: 16px;
    padding: 34px 24px 24px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    background: var(--surface);
  }
  .hero {
    text-align: center;
  }
  .logo {
    width: 58px;
    height: 58px;
    margin: 0 auto 10px;
    border-radius: 16px;
    display: grid;
    place-items: center;
    color: #fff;
    background: linear-gradient(135deg, var(--accent-2), var(--accent));
    box-shadow: 0 6px 20px color-mix(in srgb, var(--accent) 35%, transparent);
  }
  h1 {
    margin: 0;
    font-size: 20px;
  }
  .hero p {
    margin: 4px 0 0;
  }
  kbd {
    font: inherit;
    font-size: 12px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: 4px;
    padding: 0 5px;
  }
  .add {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 4px 4px 4px 12px;
    color: var(--muted);
    width: 100%;
    max-width: 760px;
    margin: 0 auto;
  }
  .add:focus-within {
    border-color: var(--accent);
  }
  .add input {
    flex: 1;
    border: none;
    background: none;
    box-shadow: none !important;
    padding: 5px 0;
    color: var(--text);
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }
  .seg {
    display: inline-flex;
    background: var(--surface-2);
    border-radius: 8px;
    padding: 2px;
  }
  .seg button {
    border: none;
    background: none;
    border-radius: 6px;
    padding: 4px 12px;
    color: var(--muted);
  }
  .seg button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
    font-weight: 600;
  }
  .sep {
    width: 1px;
    height: 20px;
    background: var(--border);
    margin: 0 2px;
  }
  .folder {
    max-width: 260px;
  }
  .folder span {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .par {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }
  .par b {
    min-width: 14px;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  .step {
    width: 24px;
    justify-content: center;
    font-size: 15px;
    line-height: 1;
  }
  .adv {
    margin-left: auto;
  }
  .apply {
    font-size: 12px;
    color: var(--muted);
  }
  button.link {
    border: none;
    background: none;
    color: var(--accent);
    padding: 0 4px;
    font-weight: 600;
  }
  button.link:hover {
    background: none !important;
    text-decoration: underline;
  }
  .banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border-radius: 8px;
    font-size: 12px;
    background: var(--surface-2);
  }
  .banner.warn {
    background: color-mix(in srgb, var(--warn) 15%, transparent);
    color: var(--warn);
  }
  .banner.err {
    background: color-mix(in srgb, var(--err) 14%, transparent);
    color: var(--err);
  }
  .queue {
    flex: 1;
    overflow: auto;
    padding: 12px 16px 80px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .group {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: var(--shadow);
  }
  .ghead {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
  }
  .tog :global(svg) {
    transition: transform 0.15s;
  }
  .tog.open :global(svg) {
    transform: rotate(90deg);
  }
  .gtitle {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .gtitle b {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .gtitle span {
    font-size: 12px;
  }
  .gbar {
    width: 120px;
    height: 5px;
    background: var(--bar);
    border-radius: 5px;
    overflow: hidden;
  }
  .gbar div {
    height: 100%;
    background: linear-gradient(90deg, var(--accent-2), var(--accent));
    transform-origin: left;
  }
  .gitems {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 0 8px 8px;
  }
  .gitems :global(.row) {
    border-color: transparent;
    background: var(--bg);
  }
  footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    padding: 8px 16px;
    border-top: 1px solid var(--border);
    background: var(--surface);
  }
  .stats {
    display: flex;
    gap: 14px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    flex-wrap: wrap;
  }
  .stats b {
    color: var(--text);
  }
  .stats .eta {
    color: var(--text);
    font-weight: 600;
  }
  .factions {
    display: flex;
    gap: 6px;
  }
  .dropover {
    position: fixed;
    inset: 8px;
    border: 3px dashed var(--accent);
    border-radius: 18px;
    background: color-mix(in srgb, var(--bg) 85%, transparent);
    backdrop-filter: blur(3px);
    display: grid;
    place-items: center;
    z-index: 100;
    color: var(--accent);
    pointer-events: none;
    font-size: 16px;
    font-weight: 600;
    text-align: center;
  }
  .logbox::backdrop {
    background: rgb(0 0 0 / 0.35);
  }
  .logbox {
    background: var(--surface);
    color: var(--text);
    border-radius: 12px;
    width: min(820px, 94vw);
    max-height: 80vh;
    padding: 0;
    border: 1px solid var(--border);
  }
  .logbox[open] {
    display: flex;
    flex-direction: column;
  }
  .logbox header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
  }
  .logbox pre {
    margin: 0;
    padding: 12px;
    overflow: auto;
    font-size: 11.5px;
    white-space: pre-wrap;
    word-break: break-all;
    font-family: ui-monospace, Menlo, monospace;
  }
  .toast {
    position: fixed;
    bottom: 56px;
    left: 50%;
    transform: translateX(-50%);
    background: var(--text);
    color: var(--bg);
    padding: 8px 14px;
    border-radius: 20px;
    font-size: 12px;
    z-index: 200;
    box-shadow: 0 6px 24px rgb(0 0 0 / 0.2);
    max-width: 80vw;
    animation: pop 0.15s ease-out;
  }
  @keyframes pop {
    from { transform: translate(-50%, 6px); opacity: 0; }
  }
</style>
