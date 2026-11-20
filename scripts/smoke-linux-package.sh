#!/usr/bin/env bash
# Run against an installed YouGet package inside a disposable X11 session.
set -euo pipefail

fixture=$(mktemp -d)
export XDG_DATA_HOME="$fixture/data"
export GDK_BACKEND=x11
export XDG_RUNTIME_DIR="$fixture/runtime"
app_data="$XDG_DATA_HOME/io.github.luckyshot.youget"
mkdir -p "$app_data" "$fixture/media" "$fixture/output"
mkdir -m 700 "$XDG_RUNTIME_DIR"
cleanup() {
  if [[ -n ${app_pid:-} ]]; then kill "$app_pid" 2>/dev/null || true; fi
  if [[ -n ${server_pid:-} ]]; then kill "$server_pid" 2>/dev/null || true; fi
  wait 2>/dev/null || true
  rm -rf "$fixture"
}
trap cleanup EXIT

python3 - "$app_data" "$fixture/output" <<'PY'
import json
import pathlib
import sys

settings = {
    "output_dir": sys.argv[2],
    "auto_update": False,
    "notifications": False,
}
(pathlib.Path(sys.argv[1]) / "settings.json").write_text(json.dumps(settings))
PY

ffmpeg -v error -f lavfi -i color=blue:s=160x120:d=2 \
  -f lavfi -i sine=frequency=440:duration=2 -c:v libx264 -c:a aac \
  -shortest "$fixture/media/sample.mp4"
python3 -u -m http.server 18080 --bind 127.0.0.1 --directory "$fixture/media" >"$fixture/http.log" 2>&1 &
server_pid=$!
youget >"$fixture/app.log" 2>&1 &
app_pid=$!

if ! window=$(timeout 30s xdotool search --sync --onlyvisible --name '^YouGet$'); then
  cat "$fixture/app.log"
  exit 1
fi
xdotool windowfocus --sync "${window%%$'\n'*}"
printf 'http://127.0.0.1:18080/sample.mp4' | xclip -selection clipboard -i
# The native window can be mapped before its webview has installed paste handlers.
for attempt in {1..15}; do
  if [[ -s "$app_data/queue.json" ]]; then break; fi
  if ! kill -0 "$app_pid" 2>/dev/null; then break; fi
  xdotool key ctrl+v
  sleep 2
done

if ! python3 - "$app_data/queue.json" "$fixture/output" <<'PY'
import json
import pathlib
import subprocess
import sys
import time

queue = pathlib.Path(sys.argv[1])
output = pathlib.Path(sys.argv[2])
deadline = time.monotonic() + 120
while time.monotonic() < deadline:
    items = json.loads(queue.read_text()) if queue.is_file() else []
    for item in items:
        if item["status"] == "error":
            raise SystemExit(item["error"])
        if item["status"] == "done":
            path = pathlib.Path(item["filepath"])
            assert path.parent == output and path.is_file(), path
            streams = subprocess.check_output(
                ["ffprobe", "-v", "error", "-show_entries", "stream=codec_type", "-of", "csv=p=0", path],
                text=True,
            )
            assert set(streams.splitlines()) == {"audio", "video"}, streams
            print(f"Downloaded and verified {path.name}")
            raise SystemExit(0)
    time.sleep(0.5)
raise SystemExit("Timed out waiting for the packaged app to finish the download")
PY
then
  cat "$fixture/app.log" "$fixture/http.log"
  exit 1
fi

test -x "$app_data/bin/yt-dlp"
if ! command -v deno >/dev/null; then
  test -x "$app_data/bin/deno"
  "$app_data/bin/deno" --version
fi
