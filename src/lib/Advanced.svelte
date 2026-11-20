<script lang="ts">
  import Icon from "./Icon.svelte";
  import { api } from "./api";
  import { app, flushSettings, saveSettings, toast } from "./store.svelte";
  import type { ToolInfo } from "./types";
  import { TEMPLATE_PRESETS, TEMPLATE_TOKENS, previewTemplate } from "./util";

  let { onclose }: { onclose: () => void } = $props();
  const s = $derived(app.settings!);

  let updating = $state(false);
  let tmplInput: HTMLInputElement | undefined = $state();

  const SB_CATS = [
    ["sponsor", "Sponsor"],
    ["intro", "Intro"],
    ["outro", "Outro / credits"],
    ["selfpromo", "Self-promotion"],
    ["interaction", "Subscribe reminders"],
    ["music_offtopic", "Non-music parts"],
    ["preview", "Preview / recap"],
    ["filler", "Filler"],
  ];
  const BROWSERS = ["", "chrome", "firefox", "safari", "brave", "edge", "chromium", "opera", "vivaldi"];

  const preview = $derived.by(() => {
    const ext = s.defaults.mode === "audio" ? s.defaults.audio_format : s.defaults.container;
    let name = previewTemplate(s.filename_template, ext);
    if (s.restrict_filenames) name = name.replace(/[^\w.\-/]+/g, "_");
    return name;
  });

  function insertToken(t: string) {
    const tok = `%(${t})s`;
    const el = tmplInput;
    if (!el) return;
    const start = el.selectionStart ?? s.filename_template.length;
    const end = el.selectionEnd ?? start;
    s.filename_template = s.filename_template.slice(0, start) + tok + s.filename_template.slice(end);
    saveSettings();
    queueMicrotask(() => {
      el.focus();
      el.setSelectionRange(start + tok.length, start + tok.length);
    });
  }

  function toggleCat(c: string) {
    s.sponsorblock_categories = s.sponsorblock_categories.includes(c)
      ? s.sponsorblock_categories.filter((x) => x !== c)
      : [...s.sponsorblock_categories, c];
    saveSettings();
  }

  async function update() {
    updating = true;
    try {
      await flushSettings();
      const r = await api.updateYtdlp();
      toast(r || "yt-dlp is up to date", 4000);
    } catch (e) {
      toast(String(e), 6000);
    } finally {
      updating = false;
    }
  }

  async function recheck() {
    try {
      await flushSettings();
      await api.recheckTools();
    } catch (e) {
      toast(String(e), 6000);
    }
  }

  async function pickCookies() {
    const f = await api.pickFile();
    if (f) {
      s.cookies_file = f;
      saveSettings();
    }
  }

  function toolLine(t: ToolInfo | undefined) {
    if (!t) return "";
    if (t.status === "ok") return `${t.version ?? ""}${t.managed ? " · managed by YouGet" : " · system"}`;
    return t.message ?? t.status;
  }
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <aside class="drawer" oninput={saveSettings} onchange={saveSettings}>
    <header>
      <h2>Advanced settings</h2>
      <button class="ghost" onclick={onclose} title="Close"><Icon name="x" /></button>
    </header>

    <div class="body">
      <details open>
        <summary>File naming</summary>
        <div class="field">
          <span class="lbl">Filename</span>
          <input class="mono wide" type="text" bind:value={s.filename_template} bind:this={tmplInput} spellcheck="false" />
          <div class="chips">
            {#each TEMPLATE_PRESETS as p}
              <button class="chip" onclick={() => { s.filename_template = p.value; saveSettings(); }}>{p.label}</button>
            {/each}
          </div>
          <div class="chips">
            <span class="muted small">Insert:</span>
            {#each TEMPLATE_TOKENS as t}
              <button class="chip tok" onclick={() => insertToken(t)}>{t}</button>
            {/each}
          </div>
          <div class="preview"><span class="muted">Preview:</span> <code class="selectable">{preview}</code></div>
        </div>
        <label><input type="checkbox" bind:checked={s.playlist_subfolder} /> Put playlists in their own folder</label>
        <label><input type="checkbox" bind:checked={s.playlist_numbering} /> Number playlist files (001 - …)</label>
        <label><input type="checkbox" bind:checked={s.restrict_filenames} /> ASCII-only filenames, no spaces</label>
        <div class="field row">
          <span class="lbl">If file exists</span>
          <select bind:value={s.overwrite}>
            <option value="skip">Skip</option>
            <option value="overwrite">Overwrite</option>
          </select>
        </div>
      </details>

      <details>
        <summary>Embedding & metadata</summary>
        <label><input type="checkbox" bind:checked={s.embed_thumbnail} /> Embed thumbnail / cover art</label>
        <label><input type="checkbox" bind:checked={s.embed_metadata} /> Embed metadata (title, artist, date…)</label>
        <label><input type="checkbox" bind:checked={s.embed_chapters} /> Embed chapters</label>
        <label><input type="checkbox" bind:checked={s.split_chapters} /> Split into one file per chapter</label>
      </details>

      <details>
        <summary>Subtitles</summary>
        <label><input type="checkbox" bind:checked={s.subtitles} /> Download subtitles (video only)</label>
        <fieldset disabled={!s.subtitles}>
          <div class="field row">
            <span class="lbl">Languages</span>
            <input type="text" bind:value={s.sub_langs} placeholder="en,es" spellcheck="false" />
          </div>
          <label><input type="checkbox" bind:checked={s.auto_subs} /> Include auto-generated</label>
          <label><input type="checkbox" bind:checked={s.embed_subs} /> Embed into the video file</label>
          <div class="field row">
            <span class="lbl">Format</span>
            <select bind:value={s.sub_format}><option value="srt">SRT</option><option value="vtt">VTT</option></select>
          </div>
        </fieldset>
      </details>

      <details>
        <summary>SponsorBlock</summary>
        <div class="field row">
          <span class="lbl">Segments</span>
          <select bind:value={s.sponsorblock}>
            <option value="off">Off</option>
            <option value="mark">Mark as chapters</option>
            <option value="remove">Cut them out</option>
          </select>
        </div>
        <fieldset disabled={s.sponsorblock === "off"} class="cats">
          {#each SB_CATS as [k, l]}
            <label><input type="checkbox" checked={s.sponsorblock_categories.includes(k)} onchange={() => toggleCat(k)} /> {l}</label>
          {/each}
        </fieldset>
      </details>

      <details>
        <summary>Access (age-restricted, members-only)</summary>
        <p class="muted small">Use your browser's YouTube login. The browser must be closed for Chrome-based browsers on some systems.</p>
        <div class="field row">
          <span class="lbl">Cookies from</span>
          <select bind:value={s.cookies_browser}>
            {#each BROWSERS as b}<option value={b}>{b ? b[0].toUpperCase() + b.slice(1) : "None"}</option>{/each}
          </select>
        </div>
        {#if s.cookies_browser}
          <div class="field row">
            <span class="lbl">Profile</span>
            <input type="text" bind:value={s.cookies_profile} placeholder="default" spellcheck="false" />
          </div>
        {:else}
          <div class="field row">
            <span class="lbl">Cookies file</span>
            <input type="text" class="wide mono" bind:value={s.cookies_file} placeholder="cookies.txt (Netscape format)" spellcheck="false" />
            <button onclick={pickCookies}>Browse…</button>
          </div>
        {/if}
      </details>

      <details>
        <summary>Network</summary>
        <div class="field row"><span class="lbl">Speed limit</span><input type="text" bind:value={s.rate_limit} placeholder="e.g. 5M (empty = unlimited)" spellcheck="false" /></div>
        <div class="field row"><span class="lbl">Fragments</span><input type="number" min="1" max="32" bind:value={s.concurrent_fragments} /><span class="muted small">parallel pieces per download</span></div>
        <div class="field row"><span class="lbl">Retries</span><input type="number" min="0" max="100" bind:value={s.retries} /></div>
        <div class="field row"><span class="lbl">Proxy</span><input type="text" class="wide" bind:value={s.proxy} placeholder="socks5://127.0.0.1:1080" spellcheck="false" /></div>
      </details>

      <details>
        <summary>Playlists & channels</summary>
        <div class="field row"><span class="lbl">Items</span><input type="text" bind:value={s.playlist_items} placeholder="all  (e.g. 1:10, -5:, 1,3,7)" spellcheck="false" /></div>
        <label><input type="checkbox" bind:checked={s.playlist_reverse} /> Reverse order</label>
        <label><input type="checkbox" bind:checked={s.playlist_random} /> Random order</label>
        <label><input type="checkbox" bind:checked={s.use_archive} /> Skip videos already downloaded before (download archive)</label>
      </details>

      <details>
        <summary>Formats</summary>
        <div class="field row">
          <span class="lbl">Video codec</span>
          <select bind:value={s.codec_pref}>
            <option value="any">Automatic (best for container)</option>
            <option value="h264">Prefer H.264 (most compatible)</option>
            <option value="vp9">Prefer VP9</option>
            <option value="av1">Prefer AV1 (smallest)</option>
          </select>
        </div>
        <label><input type="checkbox" bind:checked={s.force_keyframes} /> Exact cuts for clips (re-encodes, slower)</label>
      </details>

      <details>
        <summary>yt-dlp & tools</summary>
        <div class="tools">
          {#each [["yt-dlp", app.tools?.ytdlp], ["ffmpeg", app.tools?.ffmpeg], ["Deno", app.tools?.deno]] as [name, t]}
            {@const ti = t as ToolInfo | undefined}
            <div class="tool">
              <span class="dot {ti?.status}"></span>
              <b>{name}</b>
              <span class="muted small selectable" title={ti?.path ?? ""}>{toolLine(ti)}</span>
            </div>
          {/each}
        </div>
        <div class="field row">
          <span class="lbl">Channel</span>
          <select bind:value={s.channel}>
            <option value="stable">Stable</option>
            <option value="nightly">Nightly (fixes land sooner)</option>
          </select>
          <button onclick={update} disabled={updating || !!s.custom_ytdlp}>{updating ? "Updating…" : "Update now"}</button>
        </div>
        <label><input type="checkbox" bind:checked={s.auto_update} /> Check for yt-dlp updates daily</label>
        <div class="field row">
          <span class="lbl">Custom yt-dlp</span>
          <input type="text" class="wide mono" bind:value={s.custom_ytdlp} placeholder="Leave empty to let YouGet manage it" spellcheck="false" />
        </div>
        <button onclick={recheck}>Re-check tools</button>
      </details>

      <details>
        <summary>App & extra arguments</summary>
        <label><input type="checkbox" bind:checked={s.notifications} /> Notify when downloads finish or fail</label>
        <div class="field">
          <span class="lbl">Extra yt-dlp arguments (added to every download)</span>
          <textarea class="mono" rows="2" bind:value={s.extra_args} placeholder="--limit-rate 2M --geo-bypass" spellcheck="false"></textarea>
        </div>
        <p class="muted small">YouGet {app.version}</p>
      </details>
    </div>
  </aside>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.25);
    z-index: 40;
  }
  .drawer {
    position: absolute;
    right: 0;
    top: 0;
    bottom: 0;
    width: min(460px, 94vw);
    background: var(--bg);
    border-left: 1px solid var(--border);
    box-shadow: -10px 0 40px rgb(0 0 0 / 0.15);
    display: flex;
    flex-direction: column;
    animation: in 0.18s ease-out;
  }
  @keyframes in {
    from { transform: translateX(30px); opacity: 0; }
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 14px 16px 10px;
  }
  h2 {
    margin: 0;
    font-size: 15px;
  }
  .body {
    overflow: auto;
    padding: 0 16px 24px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  details {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 0 12px;
  }
  details[open] {
    padding-bottom: 12px;
  }
  details > :not(summary) {
    margin-top: 8px;
  }
  summary {
    padding: 10px 0;
    font-weight: 600;
    cursor: pointer;
    list-style: none;
  }
  summary::before {
    content: "›";
    display: inline-block;
    width: 14px;
    color: var(--muted);
    transition: transform 0.15s;
  }
  details[open] summary::before {
    transform: rotate(90deg);
  }
  details > label {
    display: flex;
  }
  fieldset {
    border: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  fieldset:disabled {
    opacity: 0.5;
  }
  .cats {
    display: grid;
    grid-template-columns: 1fr 1fr;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .field.row {
    flex-direction: row;
    align-items: center;
  }
  .lbl {
    color: var(--muted);
    min-width: 92px;
  }
  input[type="number"] {
    width: 70px;
  }
  .wide {
    flex: 1;
    min-width: 0;
  }
  textarea {
    resize: vertical;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
  }
  .chip {
    padding: 2px 8px;
    font-size: 11px;
    border-radius: 20px;
  }
  .chip.tok {
    font-family: ui-monospace, Menlo, monospace;
  }
  .preview {
    background: var(--surface-2);
    border-radius: 6px;
    padding: 6px 8px;
    word-break: break-all;
  }
  .small {
    font-size: 12px;
  }
  .tools {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .tool {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .tool b {
    width: 52px;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--faint);
    flex: none;
  }
  .dot.ok {
    background: var(--ok);
  }
  .dot.missing,
  .dot.error {
    background: var(--err);
  }
  .dot.downloading,
  .dot.updating,
  .dot.checking {
    background: var(--warn);
  }
</style>
