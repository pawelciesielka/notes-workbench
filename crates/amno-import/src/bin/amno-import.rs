//! Import z wiersza poleceń: `amno-import <eksport.htm> <katalog-skarbca>`.

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Użycie: amno-import <My Data.htm> <pusty-katalog-skarbca>");
        std::process::exit(2);
    }
    let opts = amno_import::Options {
        export_html: PathBuf::from(&args[1]),
        vault: PathBuf::from(&args[2]),
        date: chrono::Local::now().format("%Y-%m-%d").to_string(),
    };
    let t = std::time::Instant::now();
    match amno_import::run(&opts, &mut |_, _| {}) {
        Ok(r) => {
            let c = &r.counts;
            println!(
                "Notatki {}/{} (Kosz {}), Foldery {}/{}, obrazy {}, załączniki {}, pliki {}/{} (nieprzypisane {})",
                c.notes_written, c.notes_in_export, c.notes_in_trash, c.folders_written, c.folders_in_export,
                c.images, c.attachments, c.files_copied, c.files_in_export, c.files_unreferenced
            );
            println!("Błędy: {}, ostrzeżenia: {}, zmiany: {}", r.errors, r.warnings, r.issues.len() - r.errors - r.warnings);
            let mut kinds = std::collections::BTreeMap::new();
            for i in &r.issues {
                *kinds.entry((i.level, i.kind)).or_insert(0) += 1;
            }
            for ((l, k), n) in kinds {
                println!("  {l:12} {k:25} {n}");
            }
            println!("Czas: {:.1?}", t.elapsed());
            std::process::exit(if r.ok { 0 } else { 1 });
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
}
