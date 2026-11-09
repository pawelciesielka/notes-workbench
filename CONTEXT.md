# Notes Workbench

Aplikacja desktopowa do notatek w plikach Markdown, następca All My Notes Organizer: przenosi z niego notatki bez strat, a w kolejnych etapach dodaje wyszukiwanie, łączenie treści przez AI i dostęp dla Claude przez MCP.

## Język

### Skarbiec

**Skarbiec**:
Katalog z Notatkami, Folderami i Załącznikami, który jest jedynym źródłem prawdy; wszystko inne w aplikacji da się z niego odtworzyć.
_Unikaj_: vault, baza, repozytorium

**Notatka**:
Pojedynczy dokument Markdown w Skarbcu, z tytułem, który może różnić się od nazwy pliku.
_Unikaj_: strona, wpis, plik (w znaczeniu domenowym)

**Folder**:
Katalog w Skarbcu grupujący Notatki i inne Foldery.
_Unikaj_: kategoria, gałąź

**Notatka folderu**:
Notatka o tej samej nazwie co Folder, w którym leży, opisująca jego zawartość; otwiera się po wybraniu Folderu i nie jest pokazywana osobno w drzewie.
_Unikaj_: indeks folderu, strona główna folderu

**Załącznik**:
Plik niebędący Notatką (obraz, PDF, dokument Office itd.), należący do jednej Notatki i przechowywany obok niej.
_Unikaj_: plik, attachment, zasób

**Kolejność**:
Ręcznie ustalona pozycja Notatki lub Folderu wśród rodzeństwa, przejęta z AMNO; ma pierwszeństwo przed sortowaniem alfabetycznym.
_Unikaj_: sortowanie, pozycja

**Kosz**:
Część Skarbca z usuniętymi Notatkami i Folderami, które można przywrócić; nie pojawia się w drzewie ani w wyszukiwaniu. Import umieszcza w nim zawartość Kosza z AMNO.
_Unikaj_: recycle bin, archiwum, usunięte

**Link zewnętrzny**:
Odnośnik z Notatki do pliku lub folderu na dysku poza Skarbcem; otwierany w Windows, nigdy nie kopiowany do Skarbca.
_Unikaj_: załącznik, skrót

### Graf wiedzy

**Link**:
Odnośnik w treści jednej Notatki do innej Notatki, wskazujący ją po nazwie pliku.
_Unikaj_: odnośnik wewnętrzny, wikilink (w znaczeniu domenowym)

**Link nierozwiązany**:
Link do Notatki, która jeszcze nie istnieje; jego kliknięcie tworzy tę Notatkę.
_Unikaj_: martwy link, pusty link

**Backlink**:
Link widziany od strony Notatki, na którą wskazuje.
_Unikaj_: link zwrotny, odwołanie

**Tag**:
Etykieta przypisana Notatce, wspólna dla wielu Notatek i będąca osobnym węzłem Grafu wiedzy.
_Unikaj_: kategoria, słowo kluczowe, etykieta

**Powiązanie**:
Zaakceptowane przez użytkownika połączenie dwóch Notatek zaproponowane przez AI, zapisane w metadanych Notatki, a nie w jej treści.
_Unikaj_: relacja, link AI, sugestia

**Propozycja powiązania**:
Znalezione przez AI, jeszcze nie zaakceptowane połączenie dwóch Notatek z krótkim uzasadnieniem; po akceptacji staje się Powiązaniem, a odrzucona nie wraca.
_Unikaj_: rekomendacja, podpowiedź

**Notatka podobna**:
Notatka o zbliżonej treści według lokalnego modelu, pokazywana przy Notatce; nie jest ani Linkiem, ani Powiązaniem i nie trafia do Grafu wiedzy.
_Unikaj_: powiązana, rekomendowana

**Graf wiedzy**:
Sieć, w której węzłami są Notatki i Tagi, a krawędziami Linki, Powiązania i przypisania Tagów; krawędzie nie mają typu.
_Unikaj_: mapa myśli, sieć notatek

### Import

**AMNO**:
All My Notes Organizer, program, z którego przenosimy notatki. Jego baza (`.ddb`) jest zaszyfrowana, więc jedynym źródłem jest Eksport AMNO.

**Eksport AMNO**:
Plik HTML z całym drzewem AMNO wraz z katalogiem plików, wygenerowany przez AMNO; nie zawiera dat, ikon ani priorytetów.
_Unikaj_: backup, zrzut

**Import**:
Jednorazowe przeniesienie Eksportu AMNO do nowego Skarbca, po którym AMNO przestaje być używany.
_Unikaj_: migracja, synchronizacja, konwersja

**Tytuł zastępczy**:
Tytuł nadany przy Imporcie Notatce, która w AMNO nie miała tytułu, wzięty z pierwszej niepustej linii jej treści.

**Raport importu**:
Zapis wszystkiego, co Import zmienił lub uprościł względem Eksportu AMNO (nowe nazwy plików, naprawione linki, ostrzeżenia), wraz z liczbami do sprawdzenia kompletności.
_Unikaj_: log, dziennik
