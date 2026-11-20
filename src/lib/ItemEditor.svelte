<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "./api";
  import type { Format, Item, ItemOptions } from "./types";
  import { AUDIO_FORMATS, AUDIO_QUALITIES, CONTAINERS, VIDEO_QUALITIES, dismissOnBackdrop, fmtBytes } from "./util";

  let { item, onclose }: { item: Item; onclose: () => void } = $props();

  let dialog: HTMLDialogElement;
  onMount(() => {
    dialog.showModal();
  });

  // Intentionally a copy: edits apply only when the user presses Save.
  // svelte-ignore state_referenced_locally
  let o: ItemOptions = $state(structuredClone($state.snapshot(item.opts)) as ItemOptions);
  let formats = $state<Format[] | null>(null);
  let loading = $state(false);
  let saving = $state(false);
  let errorMessage = $state("");
  let pickV = $state<string | null>(null);
  let pickA = $state<string | null>(null);

  // Restore picks from an existing "v+a" selector.
  if (o.format_id) {
    const [first, second] = o.format_id.split("+");
    if (o.mode === "audio") {
      pickA = first || null;
    } else {
      pickV = first || null;
      pickA = second || null;
    }
  }

  const videos = $derived((formats ?? []).filter((f) => f.vcodec && f.vcodec !== "none").sort((a, b) => (b.height ?? 0) - (a.height ?? 0) || (b.tbr ?? 0) - (a.tbr ?? 0)));
  const audios = $derived((formats ?? []).filter((f) => (!f.vcodec || f.vcodec === "none") && f.acodec && f.acodec !== "none").sort((a, b) => (b.abr ?? 0) - (a.abr ?? 0)));

  async function loadFormats() {
    loading = true;
    errorMessage = "";
    try {
      const r = await api.listFormats(item.url);
      formats = r.formats;
    } catch (e) {
      errorMessage = String(e);
    } finally {
      loading = false;
    }
  }

  function codec(c: string | null) {
    if (!c || c === "none") return "";
    return c.split(".")[0].replace("avc1", "H.264").replace("vp09", "VP9").replace("av01", "AV1").replace("mp4a", "AAC");
  }

  async function save() {
    if (formats) {
      const sel = [pickV, pickA].filter(Boolean).join("+");
      o.format_id = sel || null;
      if (sel && !pickV) o.mode = "audio";
      if (sel && pickV) o.mode = "video";
    }
    o.clip_start = o.clip_start?.trim() || null;
    o.clip_end = o.clip_end?.trim() || null;
    saving = true;
    errorMessage = "";
    try {
      await api.setItemOptions(item.id, $state.snapshot(o) as ItemOptions);
      dialog.close();
    } catch (error) {
      errorMessage = String(error);
    } finally {
      saving = false;
    }
  }

  function clearExact() {
    pickV = null;
    pickA = null;
    o.format_id = null;
    formats = null;
  }
</script>

<dialog bind:this={dialog} aria-labelledby="item-editor-title" onpointerdown={dismissOnBackdrop} onclose={onclose}>
  <div class="dialog">
    <h2 id="item-editor-title">{item.title}</h2>

    {#if !o.format_id && !formats}
      <div class="grid">
        <span>Type</span>
        <div class="seg">
          <button class:on={o.mode === "video"} aria-pressed={o.mode === "video"} onclick={() => (o.mode = "video")}>Video</button>
          <button class:on={o.mode === "audio"} aria-pressed={o.mode === "audio"} onclick={() => (o.mode = "audio")}>Audio</button>
        </div>
        {#if o.mode === "video"}
          <span>Quality</span>
          <select aria-label="Maximum resolution" bind:value={o.video_quality}>{#each VIDEO_QUALITIES as q}<option value={q.v}>{q.l}</option>{/each}</select>
          <span>Container</span>
          <select aria-label="Container" bind:value={o.container}>{#each CONTAINERS as q}<option value={q.v}>{q.l}</option>{/each}</select>
        {:else}
          <span>Format</span>
          <select aria-label="Audio format" bind:value={o.audio_format}>{#each AUDIO_FORMATS as q}<option value={q.v}>{q.l}</option>{/each}</select>
          {#if o.audio_format !== "flac" && o.audio_format !== "wav"}
            <span>Quality</span>
            <select aria-label="Audio quality" bind:value={o.audio_quality}>{#each AUDIO_QUALITIES as q}<option value={q.v}>{q.l}</option>{/each}</select>
          {/if}
        {/if}
      </div>
    {/if}

    <h3>Exact format</h3>
    {#if formats}
      <p class="muted">Pick a video stream, an audio stream, or both. Leave video empty for audio only.</p>
      <div class="tables">
        <div class="tbl">
          <div class="th">Video</div>
          <label class="tr"><input type="radio" bind:group={pickV} value={null} /> <span class="muted">None</span></label>
          {#each videos as f}
            <label class="tr">
              <input type="radio" bind:group={pickV} value={f.id} />
              <b>{f.height ? `${f.height}p` : f.note}</b>{#if f.fps && f.fps > 30}<span class="tag">{f.fps}fps</span>{/if}
              {#if f.hdr && f.hdr !== "SDR"}<span class="tag">{f.hdr}</span>{/if}
              <span class="muted">{codec(f.vcodec)} · {f.ext}{f.acodec && f.acodec !== "none" ? " · +audio" : ""}</span>
              <span class="size">{fmtBytes(f.size)}</span>
            </label>
          {/each}
        </div>
        <div class="tbl">
          <div class="th">Audio</div>
          <label class="tr"><input type="radio" bind:group={pickA} value={null} /> <span class="muted">None</span></label>
          {#each audios as f}
            <label class="tr">
              <input type="radio" bind:group={pickA} value={f.id} />
              <b>{f.abr ? `${Math.round(f.abr)}k` : f.note}</b>
              <span class="muted">{codec(f.acodec)} · {f.ext}{f.lang ? ` · ${f.lang}` : ""}</span>
              <span class="size">{fmtBytes(f.size)}</span>
            </label>
          {/each}
        </div>
      </div>
      <button class="link" onclick={clearExact}>Use simple quality settings instead</button>
    {:else if o.format_id}
      <p>Using format <code>{o.format_id}</code>. <button class="link" onclick={loadFormats}>Change…</button> <button class="link" onclick={clearExact}>Reset</button></p>
    {:else}
      <p class="muted">Choose exact streams (resolution, codec, HDR, language) for this video.</p>
      <button onclick={loadFormats} disabled={loading}>{loading ? "Loading formats…" : "List available formats…"}</button>
    {/if}

    <h3>Clip</h3>
    <p class="muted">Download only part of the video. Times like <code>1:30</code> or <code>90</code>. Leave empty for the whole video.</p>
    <div class="clip">
      <input type="text" aria-label="Clip start" placeholder="Start (0:00)" bind:value={o.clip_start} />
      <span>→</span>
      <input type="text" aria-label="Clip end" placeholder="End (end)" bind:value={o.clip_end} />
    </div>

    {#if errorMessage}<p role="alert">{errorMessage}</p>{/if}
    <div class="foot">
      <button onclick={() => dialog.close()}>Cancel</button>
      <button class="primary" onclick={save} disabled={loading || saving}>Save</button>
    </div>
  </div>
</dialog>

<style>
  dialog::backdrop {
    background: rgb(0 0 0 / 0.35);
  }
  dialog {
    color: var(--text);
    padding: 0;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 14px;
    width: min(680px, 92vw);
    max-height: 88vh;
    overflow: auto;
    box-shadow: 0 20px 60px rgb(0 0 0 / 0.3);
  }
  .dialog { padding: 18px 20px; }
  h2 {
    margin: 0 0 12px;
    font-size: 15px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  h3 {
    margin: 18px 0 4px;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
  }
  p {
    margin: 4px 0 8px;
  }
  .grid {
    display: grid;
    grid-template-columns: 90px 1fr;
    gap: 8px 12px;
    align-items: center;
    max-width: 360px;
  }
  .seg {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: 7px;
    overflow: hidden;
    width: max-content;
  }
  .seg button {
    border: none;
    border-radius: 0;
    padding: 4px 14px;
  }
  .seg button.on {
    background: var(--accent);
    color: #fff;
  }
  .tables {
    display: grid;
    grid-template-columns: 1.4fr 1fr;
    gap: 10px;
  }
  .tbl {
    border: 1px solid var(--border);
    border-radius: 8px;
    max-height: 260px;
    overflow: auto;
  }
  .th {
    position: sticky;
    top: 0;
    background: var(--surface-2);
    padding: 5px 8px;
    font-weight: 600;
    font-size: 12px;
  }
  .tr {
    display: flex;
    padding: 4px 8px;
    gap: 6px;
    border-top: 1px solid var(--border);
    font-size: 12px;
  }
  .tr:hover {
    background: var(--surface-2);
  }
  .tag {
    font-size: 10px;
    background: var(--accent-soft);
    color: var(--accent-text);
    border-radius: 3px;
    padding: 0 4px;
  }
  .size {
    margin-left: auto;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .clip {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .clip input {
    width: 120px;
  }
  .foot {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 20px;
  }
  button.link {
    border: none;
    background: none;
    color: var(--accent-text);
    padding: 0;
    margin-top: 6px;
  }
  button.link:hover {
    background: none !important;
    text-decoration: underline;
  }
</style>
