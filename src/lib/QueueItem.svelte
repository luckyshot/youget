<script lang="ts">
  import Icon from "./Icon.svelte";
  import { api } from "./api";
  import { app, toast } from "./store.svelte";
  import type { Item } from "./types";
  import { fmtBytes, fmtDuration, fmtEta, fmtSpeed, optsLabel } from "./util";

  let {
    item,
    onedit,
    onlog,
    ondragstart,
    ondrop,
    nested = false,
  }: {
    item: Item;
    onedit: (it: Item) => void;
    onlog: (it: Item, kind: "log" | "cmd") => void;
    ondragstart: (e: DragEvent, id: string) => void;
    ondrop: (e: DragEvent, id: string) => void;
    nested?: boolean;
  } = $props();

  let menu = $state(false);
  let menuButton: HTMLButtonElement | undefined = $state();
  let over = $state(false);
  let thumbFailed = $state(false);

  const active = $derived(item.status === "downloading" || item.status === "processing");
  const editable = $derived(["queued", "paused", "error", "canceled"].includes(item.status));

  const statusText = $derived.by(() => {
    switch (item.status) {
      case "fetching":
        return "Fetching info…";
      case "queued": {
        const wait = (item.retry_at ?? 0) - app.now;
        if (wait > 0) return `${item.stage || "Failed"} · retrying in ${wait}s (attempt ${(item.attempts ?? 0) + 1})`;
        return "Queued";
      }
      case "paused":
        return item.percent > 0 ? `Paused at ${Math.floor(item.percent)}%` : "Paused";
      case "done":
        return item.stage || "Done";
      case "canceled":
        return "Canceled";
      case "error":
        return item.error ?? "Failed";
      case "processing":
        return item.stage || "Processing…";
      case "downloading": {
        if (item.downloaded <= 0) return item.stage || "Starting…";
        const parts = [`${Math.floor(item.percent)}%`];
        const sp = fmtSpeed(item.speed);
        if (sp) parts.push(sp);
        const eta = fmtEta(item.eta);
        if (eta) parts.push(`${eta} left`);
        return parts.join(" · ");
      }
    }
  });

  async function copy(text: string, what: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast(`${what} copied`);
    } catch {
      toast("Could not copy to clipboard");
    }
  }

  async function act(fn: () => Promise<unknown>) {
    menu = false;
    try {
      await fn();
    } catch (e) {
      toast(String(e));
    }
  }

  function closeMenu(restoreFocus = false) {
    menu = false;
    if (restoreFocus) menuButton?.focus();
  }

  function onWindowClick(e: MouseEvent) {
    if (menu && !(e.target as HTMLElement).closest(".menu-wrap")) menu = false;
  }
</script>

<svelte:window onclick={onWindowClick} onkeydown={(event) => {
  if (event.key === "Escape" && menu) closeMenu(true);
}} />

<div
  class="row {item.status}"
  class:nested
  class:over
  role="listitem"
  draggable={!active}
  ondragstart={(e) => ondragstart(e, item.id)}
  ondragover={(e) => {
    if (e.dataTransfer?.types.includes("application/x-youget-item")) {
      e.preventDefault();
      e.stopPropagation();
      over = true;
    }
  }}
  ondragleave={() => (over = false)}
  ondrop={(e) => {
    over = false;
    ondrop(e, item.id);
  }}
  ondblclick={() => item.status === "done" && act(() => api.openFile(item.id))}
>
  <div class="thumb">
    {#if item.thumbnail && !thumbFailed}
      <img src={item.thumbnail} alt="" loading="lazy" onerror={() => (thumbFailed = true)} />
    {:else}
      <div class="ph"><Icon name={item.opts.mode === "audio" ? "music" : "film"} size={20} /></div>
    {/if}
    {#if item.duration}<span class="dur">{fmtDuration(item.duration)}</span>{/if}
    {#if item.status === "done"}<span class="badge ok"><Icon name="check" size={14} /></span>{/if}
  </div>

  <div class="main">
    <div class="title" title={item.title}>
      {#if item.group_index != null && nested}<span class="idx">{item.group_index}.</span>{/if}
      {item.title}
    </div>
    <div class="meta">
      {#if item.uploader}<span>{item.uploader}</span>{/if}
      <span class="chip" class:audio={item.opts.mode === "audio"}>{optsLabel(item.opts)}</span>
      {#if item.opts.clip_start || item.opts.clip_end}
        <span class="chip"><Icon name="clip" size={10} /> {item.opts.clip_start || "0:00"}–{item.opts.clip_end || "end"}</span>
      {/if}
      {#if item.total}<span>{active && item.status === "downloading" ? `${fmtBytes(item.downloaded)} / ` : ""}{fmtBytes(item.total)}</span>{/if}
    </div>
    {#if item.status !== "done" && item.status !== "error" && item.status !== "canceled"}
      <div class="bar" role="progressbar" aria-label={item.title} aria-valuenow={Math.round(item.percent)}>
        <div class="fill" class:live={active} style:width="{item.percent}%"></div>
      </div>
    {/if}
    <div class="status" class:err={item.status === "error"} title={item.status === "error" ? item.error : undefined}>
      {#if item.status === "downloading" && item.stage && item.downloaded > 0}<span class="stage">{item.stage}</span>{/if}
      {statusText}
    </div>
  </div>

  <div class="actions">
    {#if item.status === "downloading" || item.status === "queued"}
      <button class="ghost" title="Pause" onclick={() => act(() => api.itemAction(item.id, "pause"))}><Icon name="pause" /></button>
    {:else if item.status === "paused"}
      <button class="ghost" title="Resume" onclick={() => act(() => api.itemAction(item.id, "resume"))}><Icon name="play" /></button>
    {:else if item.status === "error" || item.status === "canceled"}
      <button class="ghost" title="Retry" onclick={() => act(() => api.itemAction(item.id, "retry"))}><Icon name="retry" /></button>
    {:else if item.status === "done"}
      <button class="ghost" title="Show in folder" onclick={() => act(() => api.reveal(item.id))}><Icon name="folder" /></button>
    {/if}
    {#if ["downloading", "processing", "queued", "paused", "fetching"].includes(item.status)}
      <button class="ghost" title="Cancel" onclick={() => act(() => api.itemAction(item.id, "cancel"))}><Icon name="x" /></button>
    {:else}
      <button class="ghost" title="Remove from list" onclick={() => act(() => api.itemAction(item.id, "remove"))}><Icon name="x" /></button>
    {/if}
    <div class="menu-wrap">
      <button class="ghost" title="More" bind:this={menuButton} aria-label={`More actions for ${item.title}`} aria-expanded={menu} onclick={() => (menu = !menu)}><Icon name="more" /></button>
      {#if menu}
        <div class="menu">
          <button disabled={!editable} onclick={() => { closeMenu(true); onedit(item); }}><Icon name="edit" size={14} /> Format & clip…</button>
          {#if item.status === "done"}
            <button onclick={() => act(() => api.openFile(item.id))}><Icon name="open" size={14} /> Open file</button>
            <button onclick={() => act(() => api.reveal(item.id))}><Icon name="folder" size={14} /> Show in folder</button>
            <button onclick={() => act(() => api.itemAction(item.id, "retry"))}><Icon name="retry" size={14} /> Download again</button>
          {/if}
          <button onclick={() => act(() => copy(item.url, "Link"))}><Icon name="link" size={14} /> Copy link</button>
          <button onclick={() => { closeMenu(true); onlog(item, "cmd"); }}><Icon name="terminal" size={14} /> Copy as command…</button>
          <button onclick={() => { closeMenu(true); onlog(item, "log"); }}><Icon name="terminal" size={14} /> View log</button>
          <hr />
          <button class="danger" onclick={() => act(() => api.itemAction(item.id, "remove"))}><Icon name="trash" size={14} /> Remove</button>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .row {
    display: grid;
    grid-template-columns: 112px 1fr auto;
    gap: 12px;
    align-items: center;
    padding: 8px 10px;
    border-radius: var(--radius);
    background: var(--surface);
    border: 1px solid var(--border);
    box-shadow: var(--shadow);
    position: relative;
  }
  .row.nested {
    grid-template-columns: 84px 1fr auto;
    box-shadow: none;
    padding: 6px 8px;
  }
  .row.over {
    outline: 2px solid var(--accent);
  }
  .row.done .title,
  .row.canceled .title {
    color: var(--muted);
  }
  .thumb {
    position: relative;
    aspect-ratio: 16 / 9;
    border-radius: 6px;
    overflow: hidden;
    background: var(--surface-2);
  }
  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .ph {
    display: grid;
    place-items: center;
    height: 100%;
    color: var(--faint);
  }
  .dur {
    position: absolute;
    right: 3px;
    bottom: 3px;
    background: rgb(0 0 0 / 0.75);
    color: #fff;
    font-size: 10px;
    padding: 0 4px;
    border-radius: 3px;
    font-variant-numeric: tabular-nums;
  }
  .badge.ok {
    position: absolute;
    left: 4px;
    top: 4px;
    background: var(--ok);
    color: #fff;
    border-radius: 50%;
    width: 20px;
    height: 20px;
    display: grid;
    place-items: center;
  }
  .main {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .title {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .idx {
    color: var(--faint);
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }
  .meta {
    display: flex;
    gap: 8px;
    align-items: center;
    color: var(--muted);
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    background: var(--surface-2);
    border-radius: 4px;
    padding: 0 5px;
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
  }
  .chip.audio {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .bar {
    height: 5px;
    background: var(--bar);
    border-radius: 5px;
    overflow: hidden;
    margin-top: 2px;
  }
  .fill {
    height: 100%;
    background: linear-gradient(90deg, var(--accent-2), var(--accent));
    border-radius: 5px;
    transition: width 0.25s linear;
  }
  .paused .fill {
    background: var(--faint);
  }
  /* Moving sheen on the filled part only: shows activity without faking progress. */
  .fill.live {
    background-image:
      linear-gradient(110deg, transparent 30%, rgb(255 255 255 / 0.35) 50%, transparent 70%),
      linear-gradient(90deg, var(--accent-2), var(--accent));
    background-size: 200% 100%, 100% 100%;
    animation: sheen 1.6s linear infinite;
  }
  @keyframes sheen {
    from { background-position: 150% 0, 0 0; }
    to { background-position: -50% 0, 0 0; }
  }
  .row.queued .status {
    color: var(--muted);
  }
  .status {
    font-size: 12px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .stage {
    color: var(--text);
    margin-right: 6px;
  }
  .status.err {
    color: var(--err);
  }
  .actions {
    display: flex;
    gap: 2px;
    align-items: center;
  }
  .menu-wrap {
    position: relative;
  }
  .menu {
    position: absolute;
    right: 0;
    top: 100%;
    z-index: 20;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 9px;
    box-shadow: 0 8px 30px rgb(0 0 0 / 0.18);
    padding: 4px;
    min-width: 190px;
    display: flex;
    flex-direction: column;
  }
  .menu button {
    border: none;
    background: none;
    justify-content: flex-start;
    padding: 6px 8px;
    border-radius: 6px;
  }
  .menu button.danger {
    color: var(--err);
  }
  .menu hr {
    border: none;
    border-top: 1px solid var(--border);
    margin: 4px 2px;
  }
</style>
