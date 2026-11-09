# Notatki jako Markdown z wstawkami HTML

Źródłem prawdy są pliki Markdown, ale AMNO zapisuje formatowanie, którego Markdown nie wyraża: kolor tekstu, kolor tła, wyrównanie, tabele ze scalonymi komórkami. Żeby Import niczego nie zgubił, to formatowanie trafia do Notatki jako wstawki HTML (`<span style="color:…">`, `<mark>`, `<div align>`), a domyślny font (Segoe UI, Corbel, Calibri) pomijamy. Kod pisany fontem o stałej szerokości (Courier New, Consolas) staje się kodem Markdown.

## Rozważone opcje

- Czysty Markdown: odrzucony, bo gubi kolory i podświetlenia, których użytkownik używa jako znaczników.
- Markdown z wstawkami HTML plus nietknięty Eksport AMNO zarchiwizowany w Skarbcu: odrzucony przez użytkownika. Wierność ma zapewnić sama konwersja, sprawdzana Raportem importu.

## Konsekwencje

Edytor i renderer muszą obsługiwać inline HTML. Notatki pozostają czytelne w Obsidianie, który renderuje te same wstawki.
