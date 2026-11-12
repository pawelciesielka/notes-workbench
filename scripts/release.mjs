// Wydanie: node scripts/release.mjs 0.2.0 ["Opis zmian"]
// Ustawia wersję, buduje podpisany instalator i składa w release/v<wersja>/ pliki do wgrania na GitHub:
// instalator i latest.json (z niego program sprawdza aktualizacje).
// Klucz podpisu: %USERPROFILE%\.tauri\notes-workbench.key albo zmienna TAURI_SIGNING_PRIVATE_KEY.
import { execSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, copyFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const REPO = "pawelciesielka/notes-workbench";
const [version, notes = ""] = process.argv.slice(2);
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
  console.error('Użycie: node scripts/release.mjs <wersja, np. 0.2.0> ["opis zmian"]');
  process.exit(1);
}

const edit = (file, re, to) => {
  const s = readFileSync(file, "utf8");
  if (!re.test(s)) throw new Error(`Nie znaleziono wersji w ${file}`);
  writeFileSync(file, s.replace(re, to));
};
edit("package.json", /("version":\s*")[^"]+"/, `$1${version}"`);
edit("package-lock.json", /("name":\s*"notes-workbench",\s*"version":\s*")[^"]+"/g, `$1${version}"`);
edit("src-tauri/tauri.conf.json", /("version":\s*")[^"]+"/, `$1${version}"`);
// Wszystkie crate'y mają wspólną wersję (serwer MCP podaje ją klientom AI).
for (const f of ["src-tauri", "crates/notes-core", "crates/notes-mcp", "crates/amno-import"]) {
  edit(`${f}/Cargo.toml`, /^(version\s*=\s*")[^"]+"/m, `$1${version}"`);
}

const env = { ...process.env };
if (!env.TAURI_SIGNING_PRIVATE_KEY) {
  const keyFile = join(homedir(), ".tauri", "notes-workbench.key");
  if (!existsSync(keyFile)) throw new Error(`Brak klucza podpisu: ${keyFile}`);
  env.TAURI_SIGNING_PRIVATE_KEY = readFileSync(keyFile, "utf8");
}
env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ??= "";
env.PATH = `${join(homedir(), ".cargo", "bin")}${process.platform === "win32" ? ";" : ":"}${env.PATH}`;

execSync("npm run tauri build", { stdio: "inherit", env });

// Workspace Cargo: wynik budowania w target/ w katalogu głównym repozytorium.
const nsis = "target/release/bundle/nsis";
const built = `Notes Workbench_${version}_x64-setup.exe`;
// GitHub zamienia spacje w nazwach plików wydania na kropki: od razu używamy takiej nazwy.
const asset = built.replaceAll(" ", ".");
const out = join("release", `v${version}`);
mkdirSync(out, { recursive: true });
copyFileSync(join(nsis, built), join(out, asset));
const signature = readFileSync(join(nsis, `${built}.sig`), "utf8").trim();
const manifest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": { signature, url: `https://github.com/${REPO}/releases/download/v${version}/${asset}` },
  },
};
writeFileSync(join(out, "latest.json"), JSON.stringify(manifest, null, 2) + "\n");

console.log(`
Gotowe: ${out}
  ${asset}
  latest.json

Dalej:
  1. git commit -am "Wersja ${version}" && git push
  2. gh release create v${version} ${out}/${asset} ${out}/latest.json --title "Notes Workbench ${version}" --notes "..."
     (nie jako pre-release: program czyta „latest”).
`);
