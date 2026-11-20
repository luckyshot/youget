// Sets the app version everywhere it is declared.
// Usage: node scripts/set-version.mjs 1.2.3   (a leading "v" is stripped)
import { readFileSync, writeFileSync } from "node:fs";

const v = (process.argv[2] ?? "").replace(/^v/, "");
if (!/^\d+\.\d+\.\d+([-+][\w.]+)?$/.test(v)) {
  console.error(`Invalid version: "${process.argv[2] ?? ""}"`);
  process.exit(1);
}

const edit = (file, fn) => writeFileSync(file, fn(readFileSync(file, "utf8")));

edit("package.json", (s) => s.replace(/"version":\s*"[^"]*"/, `"version": "${v}"`));
edit("package-lock.json", (s) => {
  const lock = JSON.parse(s);
  lock.version = v;
  lock.packages[""].version = v;
  return JSON.stringify(lock, null, 2) + "\n";
});
edit("src-tauri/Cargo.lock", (s) =>
  s.replace(/(\[\[package\]\]\nname = "youget"\nversion = )"[^"]*"/, `$1"${v}"`),
);
edit("src-tauri/Cargo.toml", (s) => s.replace(/^version = "[^"]*"/m, `version = "${v}"`));
// pacman versions cannot contain "-"
edit("packaging/arch/PKGBUILD", (s) => s.replace(/^pkgver=.*$/m, `pkgver=${v.replace(/[-+]/g, "_")}`));
console.log(`Version set to ${v}`);
