use notify::{Config, Event, RecommendedWatcher, Watcher};
use std::path::Path;
use tokio::{io, sync::mpsc};

pub async fn watch() -> io::Result<()> {
    let (tx, mut rx) = mpsc::channel(100);

    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<Event>| {
            let _ = tx.blocking_send(res);
        },
        Config::default(),
    )
    .unwrap();

    let path = Path::new("synche-files");
    watcher
        .watch(path, notify::RecursiveMode::Recursive)
        .unwrap();

    println!("Watching for file changes...");

    while let Some(res) = rx.recv().await {
        match res {
            Ok(event) => handle_event(event),
            Err(err) => {
                eprintln!("Watch error: {}", err);
            }
        }
    }
    Ok(())
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
