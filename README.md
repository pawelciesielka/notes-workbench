# Notes Workbench

Desktopowa aplikacja (Windows) do notatek w plikach Markdown, następca All My Notes Organizer.
Słownik pojęć: [CONTEXT.md](CONTEXT.md). Decyzje architektoniczne: [docs/adr](docs/adr).

Etap 1: Import z AMNO i przeglądarka Skarbca.
Etap 2: edycja Notatek (Ctrl+E, autozapis), Linki `[[…]]` z podpowiadaniem, Backlinki, Tagi (`#tag` w treści
albo `tags:` we frontmatterze), Graf wiedzy (cały Skarbiec albo wokół Notatki), nowa Notatka, Kosz z przywracaniem.
Etap 3: wyszukiwanie (Ctrl+Shift+F) po słowach (SQLite FTS5) i po znaczeniu (lokalny model multilingual-e5-small,
pobierany raz do `%LOCALAPPDATA%\eu.ciesielka.notesworkbench\modele`; wektory w `…\wektory`, poza Skarbcem),
„Podobne treścią” pod Notatką.

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
Wynik trafia do `.notes-workbench/import/raport.json` i `raport.md` w Skarbcu (w aplikacji: „Raport importu”).

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
- `src-tauri/src/`: komendy aplikacji i protokół `vault://` (obrazy, PDF ze Skarbca)
  - `vault.rs`: drzewo, odczyt i zapis Notatek, Kosz
  - `index.rs`: indeks Linków, Backlinków i Tagów w pamięci (pomija kod i HTML), Graf wiedzy
  - `search.rs`: wyszukiwanie pełnotekstowe (FTS5 w pamięci, synchronizacja po dacie modyfikacji)
  - `semantic.rs`: wektory fragmentów Notatek, wyszukiwanie po znaczeniu, Notatki podobne
- `src/`: UI (React): drzewo, widok i edytor Notatki (CodeMirror), Tagi, Graf wiedzy (force-graph), Raport, kreator Importu
