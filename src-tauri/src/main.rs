#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `notes-workbench.exe --mcp [--vault <katalog>]`: serwer MCP dla Claude/Codex na stdin/stdout, bez okna.
    // Klient MCP uruchamia proces z potokami, więc mimo podsystemu GUI wejście i wyjście działają.
    if std::env::args().nth(1).as_deref() == Some("--mcp") {
        notes_mcp::serve(std::env::args().skip(1));
        return;
    }
    // `--daily-brief`: start z Harmonogramu zadań (logowanie, odblokowanie). Okno tylko przy pierwszym wejściu
    // danego dnia, gdy jest dzisiejszy brief; inaczej proces kończy się od razu, bez okna.
    if std::env::args().nth(1).as_deref() == Some(notes_workbench_lib::BRIEF_ARG) {
        if notes_workbench_lib::brief_should_show() {
            notes_workbench_lib::run(true);
        }
        return;
    }
    notes_workbench_lib::run(false)
}
