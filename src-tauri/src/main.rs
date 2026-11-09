#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `notes-workbench.exe --mcp [--vault <katalog>]`: serwer MCP dla Claude/Codex na stdin/stdout, bez okna.
    // Klient MCP uruchamia proces z potokami, więc mimo podsystemu GUI wejście i wyjście działają.
    if std::env::args().nth(1).as_deref() == Some("--mcp") {
        notes_mcp::serve(std::env::args().skip(1));
        return;
    }
    notes_workbench_lib::run()
}
