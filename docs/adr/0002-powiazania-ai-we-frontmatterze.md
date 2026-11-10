# Powiązania od AI zapisywane we frontmatterze, tylko po akceptacji

AI ma łączyć Notatki w Graf wiedzy, ale treść Notatki należy wyłącznie do użytkownika. Dlatego Powiązanie trafia do frontmattera Notatki (`powiazane: ["[[…]]"]`), a nie do jej treści, i dopiero wtedy, gdy użytkownik zaakceptuje Propozycję powiązania. Dzięki temu zawsze widać, co dodało AI, i można to usunąć bez ingerencji w tekst. Linki pisane ręcznie w treści używają składni `[[…]]`, zgodnej z Obsidianem.

## Rozważone opcje

- Linki `[[…]]` wstawiane przez AI w treść: odrzucone, bo mieszają wkład AI z tekstem użytkownika.
- Sekcja `## Powiązane` na końcu treści: odrzucona z tego samego powodu, choć jest bardziej widoczna w innych edytorach.
- Powiązania tylko w indeksie aplikacji: odrzucone, bo łamią zasadę, że Skarbiec jest jedynym źródłem prawdy.

## Konsekwencje

Graf wiedzy i backlinki muszą czytać zarówno Linki z treści, jak i pole `powiazane` z frontmattera.

Propozycje (oczekujące i odrzucone) leżą w Skarbcu, w `.notes-workbench/propozycje.json`: odrzucenie to decyzja użytkownika, więc musi przetrwać przeinstalowanie aplikacji i synchronizować się z notatkami. Zaakceptowana Propozycja znika z kolejki, bo jej śladem jest wpis we frontmatterze.

Zamiast wbudowanego modelu językowego Propozycje zgłasza Claude przez serwer MCP (`propose_relations`, `propose_tags`). Serwer nie zmienia Notatek, tylko kolejkę; akceptacja odbywa się w aplikacji. Ta sama kolejka posłuży, jeśli kiedyś dojdzie wbudowane AI.

Uzupełnienie (2026-11-10): na prośbę użytkownika AI może też proponować nowe Notatki i zmiany treści (`propose_note`, `propose_edit`). Zasada zostaje ta sama, tylko silniej: treść zmienia się wyłącznie po akceptacji, przy której użytkownik widzi pełny tekst albo różnice. Propozycja zmiany pamięta treść, od której wyszła; jeśli Notatka zmieniła się w międzyczasie, akceptacja jej nie nadpisze. Powiązania nadal trafiają tylko do frontmattera.
