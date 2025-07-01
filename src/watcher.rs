use notify::{Event, Watcher};
use std::{path::Path, sync::mpsc};

pub fn watch() {
    let (tx, rx) = mpsc::channel();

    let mut watcher = notify::recommended_watcher(tx).unwrap();

    let path = Path::new("synche-files");
    watcher
        .watch(path, notify::RecursiveMode::Recursive)
        .unwrap();

    println!("Watching for file changes...");

    for res in rx {
        match res {
            Ok(event) => handle_event(event),
            Err(err) => {
                eprintln!("Watch error: {}", err);
            }
        }
    }
}

fn handle_event(e: Event) {
    for path in e.paths {
        if let Some(file_name) = path.file_name().and_then(|f| f.to_str()) {
            println!("File changed: {}", file_name);
        } else {
            println!("Couldn't extract file name from path: {:?}", path);
        }
    }
}
