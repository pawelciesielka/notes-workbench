# Integracja z Jarvisem: liczenie w Pythonie, pliki w Rust, autostart zamiast usługi

Notes Workbench pokazuje zadania i dzień z Jarvisa (`C:\WORK\jarvis\jarvis.py`) i pozwala nim sterować.
Wszystko, co Jarvis liczy (kategorie aktywności, dopasowanie okien do zadań, bezczynność, zdarzenia strażnika),
aplikacja bierze z jednej komendy `jarvis.py status --json`, zamiast przepisywać te reguły do Rusta: jedna
implementacja, więc raport w aplikacji, raport HTML i `status.md` dla porannej rutyny zawsze się zgadzają.

Edycję plików (`zadania.md`, wybrane pola `config.json`) aplikacja robi sama, bo CLI Jarvisa nie ma komend do
zmiany priorytetu, daty czy tagów, a `done "fragment"` może trafić w niewłaściwe zadanie. Linia zadania jest
wskazywana po pełnej treści (tak jak w Jarvisie); gdy plik zmienił się w międzyczasie, zapis jest odrzucany.
Fokus, start, stop i autostart idą przez CLI, żeby Jarvis prowadził własny dziennik (`guard.jsonl`) i PID.

Jarvis startuje przy logowaniu (skrót w folderze Autostart), a nie jako usługa Windows: usługa działa w sesji 0
bez pulpitu i nie widzi aktywnego okna ani nie pokaże okien strażnika.

## Rozważone opcje

- Całość w Rust (czytanie `data/*.jsonl` i liczenie kategorii): odrzucone, dwie kopie tych samych reguł.
- Całość przez CLI (nowe komendy `edit`, `config`): odrzucone, bo każda zmiana w liście uruchamiałaby Pythona
  i rozbudowywała jednoplikowy skrypt o logikę potrzebną tylko aplikacji.
- Usługa Windows albo zadanie Harmonogramu: usługa niemożliwa (sesja 0), zadanie Harmonogramu możliwe,
  ale Jarvis ma już autostart przez skrót.

## Konsekwencje

Format `status --json` jest kontraktem między Jarvisem a aplikacją; jego zmiana wymaga zmiany w obu miejscach.
Aplikacja potrzebuje ścieżek do Jarvisa i do prawdziwego interpretera Pythona (nie shimu pyenv).
