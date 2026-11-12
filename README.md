# Notes Workbench

Desktopowa aplikacja (Windows) do notatek w plikach Markdown, następca All My Notes Organizer.
Słownik pojęć: [CONTEXT.md](CONTEXT.md). Decyzje architektoniczne: [docs/adr](docs/adr).

Etap 1: Import z AMNO i przeglądarka Skarbca.
Etap 2: edycja Notatek (Ctrl+E, autozapis), Linki `[[…]]` z podpowiadaniem, Backlinki, Tagi (`#tag` w treści
albo `tags:` we frontmatterze), Graf wiedzy (cały Skarbiec albo wokół Notatki), nowa Notatka, Kosz z przywracaniem.
Etap 3: wyszukiwanie (Ctrl+Shift+F) po słowach (SQLite FTS5) i po znaczeniu (lokalny model multilingual-e5-small,
pobierany raz do `%LOCALAPPDATA%\eu.ciesielka.notesworkbench\modele`; wektory w `…\wektory`, poza Skarbcem),
„Podobne treścią” pod Notatką.
Etap 4: serwer MCP tylko do odczytu (`crates/notes-mcp`) dla Claude: `search_notes`, `read_note`, `list_notes`,
`note_links`, `list_tags`. Notatki z Tagiem `noai` (przycisk „Ukryj przed AI”; na Notatce folderu ukrywa cały Folder)
są dla serwera niewidoczne.
Etap 5: Claude przez MCP proponuje powiązania i Tagi (`propose_relations`, `propose_tags`, `list_proposals`);
czekają w `.notes-workbench/propozycje.json` na decyzję w widoku „Propozycje” aplikacji (ADR 0002).
Etap 6: tą samą drogą Claude proponuje nowe Notatki (`propose_note`) i zmiany treści (`propose_edit`: cała treść
albo dopisek na końcu). W widoku „Propozycje” widać treść nowej Notatki i różnice zmian; można zaznaczyć wiele
Propozycji i zaakceptować je naraz. Zmiana Notatki, która od czasu Propozycji zmieniła się na dysku, nie jest
zapisywana (konflikt), a pozostałe zaznaczone Propozycje przechodzą dalej.
Foldery: Claude proponuje nowe Foldery (`propose_folder`, opcjonalnie z treścią Notatki folderu) i zmiany ich nazw
(`propose_folder_rename`); Notatki można proponować od razu do Folderu, który dopiero czeka na akceptację.

Edytor: „Markdown” (CodeMirror z podglądem obok) albo „Wizualny” (WYSIWYG, TipTap) z paskiem formatowania:
nagłówki, pogrubienie, kolory i wyróżnienia, listy i listy zadań, tabele ze scalaniem komórek, wyrównanie, linki.
Wybór jest zapamiętywany. Edytor wizualny zapisuje Markdown tak jak Import (kolory jako `<span style>`, wyrównanie
jako `<div align>`, złożone tabele jako HTML), ale normalizuje zapis całej Notatki przy pierwszej zmianie. Gdy po
wczytaniu wynik różniłby się od oryginału (np. nieobsługiwany HTML), pokazuje ostrzeżenie; plik jest zapisywany
dopiero po zmianie treści (`src/lib/mdHtml.ts`, `src/components/VisualEditor.tsx`).

Załączniki: zrzut ekranu wklejony Ctrl+V albo pliki przeciągnięte na Notatkę trafiają do `<Notatka>.assets/`.
W edytorze Link wstawia się w miejscu kursora (albo upuszczenia), w podglądzie na końcu Notatki.

## Serwer MCP

Serwerem jest sama aplikacja: `notes-workbench.exe --mcp [--vault <katalog>]` (stdin/stdout, bez okna).
Najprościej podłączyć go w aplikacji: przycisk z wtyczką („Połącz z AI”) → Połącz przy Claude Desktop,
Claude Code albo Codex. Aplikacja kopiuje się wtedy do `%USERPROFILE%\.notes-workbench\bin\notes-workbench-mcp.exe`
(stałe miejsce poza katalogiem instalacji, którego działający serwer nie blokuje) i zmienia w konfiguracji klienta
tylko wpis `notes-workbench`, z kopią zapasową `*.przed-notes-workbench.bak`:

- Claude Desktop: `claude_desktop_config.json` (wersja ze Sklepu: `%LOCALAPPDATA%\Claude-Data\…`); potem pełny restart Claude,
  bo działający Claude potrafi nadpisać plik swoją kopią.
- Claude Code: `claude mcp add --scope user` (gdy nie ma CLI: `~/.claude.json`).
- Codex: `~/.codex/config.toml`, sekcja `[mcp_servers.notes-workbench]`.

Osobny plik serwera (`cargo build --release -p notes-mcp`) działa tak samo. Wyszukiwanie po znaczeniu działa,
gdy jest włączone w aplikacji (serwer tylko czyta model i wektory, niczego nie przelicza).

## Wymagania

- Node.js 22+, Rust (toolchain MSVC), Visual Studio Build Tools z modułem C++ (jak w pdf-workbench).

## Uruchomienie

```bash
npm install
npm run tauri dev
```

Instalator (NSIS): `npm run tauri build`, wynik w `target/release/bundle/nsis/`.

## Import z AMNO

1. W AMNO: Plik → Eksport, typ „HTML Document”. Powstaje `My Data.htm` i katalog `My Data - files`.
2. W aplikacji: „Importuj z AMNO…”, wskaż plik `.htm` i **pusty** katalog Skarbca.

Bez interfejsu (szybkie sprawdzenie konwersji):

```bash
cargo run --release -p amno-import -- "My Data.htm" "C:\Notatki-test"
```

Import nie przerywa się na błędach. Każdą Notatkę porównuje tekstowo z eksportem, a pliki sumą SHA-256.
Wynik trafia do `.notes-workbench/import/raport.json` i `raport.md` w Skarbcu; aplikacja pokazuje „Raport importu”
tylko zaraz po imporcie, później jest już tylko w tych plikach.

## Układ Skarbca

- `Folder/Notatka.md`: Notatka z frontmatterem `title`, `order` (kolejność z AMNO), `imported`.
- `Folder/Notatka.assets/`: Załączniki Notatki (`obraz-N.png`, oryginalne nazwy pozostałych plików).
- `Folder/Folder.md`: Notatka folderu, otwierana po kliknięciu Folderu.
- `.kosz/`: Kosz (zawartość Kosza AMNO).
- `.notes-workbench/`: dane aplikacji (kolejność Folderów, Raport importu).

## Struktura kodu

- `crates/amno-import/`: Import (bez Tauri, z własnym CLI)
  - `parse.rs`: podział eksportu na Foldery i Notatki
  - `model.rs`: HTML Notatki → linie ze stylami, tabele
  - `md.rs`: zapis Markdown z wstawkami HTML (ADR 0001)
  - `verify.rs`: porównanie tekstu, obrazów i odnośników
  - `lib.rs`: zapis Skarbca, Załączniki, Kosz, Raport importu
- `crates/notes-core/`: wspólny rdzeń aplikacji i serwera MCP
  - `vault.rs`: drzewo, odczyt i zapis Notatek, Kosz
  - `index.rs`: indeks Linków, Backlinków i Tagów w pamięci (pomija kod i HTML), Graf wiedzy, `#noai`
  - `search.rs`: wyszukiwanie pełnotekstowe (FTS5 w pamięci, synchronizacja po dacie modyfikacji)
  - `semantic.rs`: wektory fragmentów Notatek, wyszukiwanie po znaczeniu, Notatki podobne
  - `proposals.rs`: kolejka Propozycji powiązań i Tagów (dodanie, akceptacja, odrzucenie)
- `crates/notes-mcp/`: serwer MCP (stdio, JSON-RPC), tylko do odczytu
- `src-tauri/src/`: komendy aplikacji i protokół `vault://` (obrazy, PDF ze Skarbca)
- `src/`: UI (React): drzewo, widok i edytor Notatki (CodeMirror), Tagi, Graf wiedzy (force-graph), Raport, kreator Importu

## Aktualizacje i wydania

Program kilka sekund po starcie sprawdza `latest.json` z najnowszego wydania na GitHubie (wtyczka updater Tauri)
i pokazuje pasek „Dostępna nowa wersja”. W stopce panelu jest wersja programu i „Sprawdź aktualizacje”.
Instalator aktualizacji musi być podpisany kluczem wydawcy; klucz publiczny jest w `src-tauri/tauri.conf.json`,
prywatny w `%USERPROFILE%\.tauri\notes-workbench.key` (poza repozytorium; jego utrata oznacza, że zainstalowane
kopie nie przyjmą kolejnych aktualizacji).

Nowe wydanie:

```
node scripts/release.mjs 0.2.0 "Opis zmian"
git commit -am "Wersja 0.2.0" && git push
gh release create v0.2.0 release/v0.2.0/* --title "Notes Workbench 0.2.0" --notes "..."
```

Skrypt ustawia wersję we wszystkich manifestach, buduje podpisany instalator i składa w `release/v<wersja>/`
instalator i `latest.json`. Wydanie nie może być oznaczone jako pre-release, bo program czyta „latest”.

## Licencja

MIT, zob. [LICENSE](LICENSE).
