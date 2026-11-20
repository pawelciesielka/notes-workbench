# Jarvis w Notes Workbench

Jarvis (`C:\WORK\jarvis\jarvis.py`) to lokalny tracker aktywności i zadań, napisany w Pythonie bez zależności
poza biblioteką standardową. Notes Workbench jest dla niego wygodną nakładką: pokazuje zadania i podsumowanie
dnia, pozwala nimi zarządzać i konfigurować Jarvisa bez wiersza poleceń. Decyzja o podziale pracy: [ADR 0003](adr/0003-integracja-z-jarvisem.md).

## Cel

Pilnowanie zadań przy ADHD. Użytkownik wybiera **jedno zadanie w fokusie**. Jarvis co 2 s sprawdza aktywne
okno i liczy czas poza fokusem. Po 5 min dostaje się powiadomienie, a po 10 min pełnoekranowe okno strażnika
z listą zadań, które zamyka się tylko decyzją (wracam, inne zadanie, zrobione, przerwa…). Strażnik działa
w godzinach pracy (domyślnie pn–pt 8–18). W aplikacji widać, co jest w fokusie, ile czasu poszło na które
zadanie i ile razy uwaga odpłynęła.

## Przepływ danych

```
rutyna Claude „Przygotowanie do spotkań” (chmura, 6:30)
   │  pisze Zadania/inbox/RRRR-MM-DD.md
   ▼
Jarvis (co 5 min) ── dopisuje nowe zadania ──► Zadania/zadania.md ◄── Notes Workbench (lista, edycja)
   │                                              │
   ├─ zapisuje Zadania/status.md ─► czyta go rano rutyna (zaległe, zrobione, statystyka dnia)
   ├─ data/RRRR-MM-DD.jsonl, guard.jsonl, state.json ─► Notes Workbench (podsumowanie dnia, fokus)
```

Folder `Zadania` leży na Google Drive (`C:\Dane` to lustro Mojego dysku) w `Dziennik\Daily_brief`, bo tylko
ten folder jest udostępniony kontu, którego używa rutyna w chmurze.

## Źródła danych

| Plik | Kto pisze | Kto czyta | Zawartość |
|---|---|---|---|
| `…\Daily_brief\Zadania\zadania.md` | użytkownik, Jarvis (import z `inbox/`, „zrobione”), Notes Workbench | Jarvis, Notes Workbench | rejestr zadań, linia `- [ ] Nazwa !1 @2026-11-20 #pzu #krygier`; `[x]` = zrobione; `!1/!2/!3` priorytet (domyślnie 3); `@data` dzień (bez daty: codziennie do zrobienia); `#słowa` dopasowują zadanie do tytułów okien |
| `…\Zadania\inbox\RRRR-MM-DD.md` | poranna rutyna | Jarvis | nowe zadania w tym samym formacie; przetworzone pliki w `C:\WORK\jarvis\imported.json` |
| `…\Zadania\status.md` | Jarvis | rutyna | otwarte i zrobione zadania, statystyka dnia |
| `C:\WORK\jarvis\data\RRRR-MM-DD.jsonl` | Jarvis | Jarvis (`status --json`) | odcinki aktywności: `s`, `e`, `d` (s), `app`, `title` |
| `C:\WORK\jarvis\guard.jsonl` | Jarvis (strażnik, `focus`) | Jarvis (`status --json`) | decyzje strażnika: `t`, `ev` (`focus`, `back`, `done`, `break`, `overlay`…), `task`, `reason` |
| `C:\WORK\jarvis\state.json` | Jarvis | Jarvis | `{"date": "…", "focus": "<nazwa zadania>"}`, zeruje się codziennie |
| `C:\WORK\jarvis\config.json` | użytkownik, Notes Workbench | Jarvis | konfiguracja; aplikacja zmienia tylko pola wymienione niżej |
| `C:\WORK\jarvis\jarvis.pid` | Jarvis (`track`) | Jarvis, Notes Workbench | PID działającego trackera |

Aplikacja nie liczy czasu ani kategorii sama: robi to Jarvis (`status --json`), żeby reguły (kategorie,
dopasowanie okien do zadań, bezczynność) były w jednym miejscu.

## Komendy CLI używane przez aplikację

Aplikacja uruchamia prawdziwy interpreter (`python.exe` z pyenv, nie shim `python.bat`) bez okna konsoli,
w katalogu Jarvisa. Ścieżki do katalogu i interpretera są w ustawieniach aplikacji (wykrywane automatycznie).

| Komenda | Po co |
|---|---|
| `jarvis.py status --json [dzień]` | wszystko do widoku: zadania z czasem, kategorie, czas aktywny, zdarzenia strażnika, fokus, stan trackera i autostartu (nowa komenda, dodana na potrzeby aplikacji) |
| `jarvis.py focus "<nazwa>"` | ustawienie fokusu; Jarvis zapisuje `state.json` i wpis w `guard.jsonl` |
| `jarvis.py start` / `stop` | start i zatrzymanie trackera, także restart po zmianie konfiguracji |
| `jarvis.py install` / `uninstall` | autostart: skrót `Jarvis.lnk` w folderze Autostart użytkownika |
| `jarvis.py report --open` | pełny raport HTML dnia w przeglądarce |

Bez CLI, bezpośrednio w Rust (`src-tauri/src/jarvis.rs`):

- edycja `zadania.md`: nowe zadanie, zrobione / niezrobione, priorytet, data, `#tagi`. Linia jest wskazywana
  po pełnej treści, nie po numerze; jeśli w międzyczasie się zmieniła (import z `inbox/`, synchronizacja Drive),
  zapis jest odrzucany i lista się odświeża. Reszta pliku (komentarze, kolejność) zostaje bez zmian.
- `config.json`: tylko `guard.enabled`, `guard.days`, `guard.hours`, `guard.warn_minutes`, `guard.overlay_minutes`
  i `tasks_file`; pozostałe klucze i ich kolejność zostają. Po zapisie działający tracker jest restartowany.

## Dlaczego autostart przy logowaniu, a nie usługa Windows

Usługa Windows działa w sesji 0, bez pulpitu użytkownika: nie widzi aktywnego okna, nie pokaże powiadomień
ani pełnoekranowego okna strażnika (tkinter). Jarvis musi więc działać w sesji zalogowanego użytkownika.
Autostart to skrót `Jarvis.lnk` w `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup` uruchamiający
`pythonw.exe jarvis.py track` (bez uprawnień administratora). Alternatywą byłoby zadanie Harmonogramu przy
logowaniu, jak przy Daily brief; skrót wybrano, bo Jarvis już go obsługuje (`install` / `uninstall`).

## W aplikacji

Ikona „Jarvis” w pasku bocznym otwiera widok z trzema zakładkami:

- **Zadania**: karta z zadaniem w fokusie, lista (dziś, zaległe, bez terminu, ostatnio zrobione) z edycją
  priorytetu, daty i tagów, „zrobione”, „ustaw fokus”, pole nowego zadania rozumiejące `!1 @data #tag`.
- **Dzień**: czas aktywny, kategorie, czas na zadaniach, odpłynięcia, zmiany fokusu, powroty, przerwy,
  oś decyzji strażnika; wybór dnia; pełny raport HTML.
- **Konfiguruj Jarvisa**: start / stop trackera, autostart, strażnik (włączony, dni, godziny, progi),
  plik rejestru zadań, ścieżki do Jarvisa i Pythona.

W widoku Daily brief, w sekcji „Dzisiaj”, pasek z fokusem i liczbą otwartych zadań prowadzi do widoku Jarvisa.

## Szczegóły techniczne

- Interpreter: ustawienie `jarvis_python` w aplikacji, a bez niego najnowszy `python.exe` z pyenv-win
  (`%USERPROFILE%\.pyenv\pyenv-win\versions`), potem z python.org (`%LOCALAPPDATA%\Programs\Python`), na końcu z PATH
  bez shimów. Kolejność ma znaczenie: `jarvis start` i `install` uruchamiają `pythonw.exe` obok tego interpretera.
- Czy tracker działa: aplikacja sprawdza, czy PID z `jarvis.pid` to proces Pythona (`tasklist`); Jarvis w
  `status --json` sprawdza nazwany mutex `Local\JarvisActivityTracker`.
- `zadania.md` Jarvis zapisuje z końcami linii CRLF (Python w Windows); aplikacja zachowuje BOM i końce linii pliku.
- `focus "<nazwa>"` najpierw szuka dokładnej nazwy, dopiero potem fragmentu (wcześniej „miku” mogło trafić w „Miku 2”).
  Fokusem może być tylko otwarte zadanie widoczne dziś: z dzisiejszą datą, zaległe albo bez terminu.
- Po zapisie konfiguracji przez aplikację `config.json` ma wcięcie 2 spacji (wcześniej 4, z PowerShella); treść
  i kolejność kluczy się nie zmieniają.
- Podgląd interfejsu poza Tauri (`npm run dev`) korzysta z przykładowych danych w `src/lib/mock.ts`.

## Stan

- [x] Plan zaakceptowany (2026-11-20), gałąź `jarvis-integracja`
- [x] `jarvis.py status --json`, dokładne dopasowanie w `focus`
- [x] Moduł Rust (`src-tauri/src/jarvis.rs`): zadania, konfiguracja, wywołania CLI; testy jednostkowe
- [x] Widok Jarvis: Zadania, Dzień, Konfiguruj Jarvisa; pasek w Daily brief
- [x] Testy (`cargo test --workspace`) i build (`tauri build --no-bundle`)
- [x] Sprawdzenie w zbudowanej aplikacji na prawdziwych danych (2026-11-20): dodanie zadania, zmiana priorytetu,
  edycja nazwy i tagów, fokus, „zrobione”, podsumowanie dnia, zapis konfiguracji z restartem trackera, walidacja
  progów, restart trackera, pasek w Daily brief. Zmiany testowe zostały cofnięte.
- [ ] Nie sprawdzone ręcznie: przełącznik „Uruchamiaj z Windows” (`install` / `uninstall`; skrót już istnieje,
  a jego przełączenie zmienia autostart użytkownika), przycisk „Pełny raport” (otwiera przeglądarkę)
- [ ] Nowa wersja aplikacji (instalator, `scripts/release.mjs`) – poza tą zmianą
