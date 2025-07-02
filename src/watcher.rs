use notify::{Config, Event, RecommendedWatcher, Watcher};
use std::path::Path;
use tokio::{
    io,
    sync::mpsc::{self, Sender},
};

pub async fn watch_files(sync_tx: Sender<String>) -> io::Result<()> {
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
            Ok(event) => handle_event(event, &sync_tx).await,
            Err(err) => {
                eprintln!("Watch error: {}", err);
            }
        }
    }
    Ok(())
}

async fn handle_event(e: Event, sync_tx: &Sender<String>) {
    for path in e.paths {
        if let Some(file_name) = path.file_name().and_then(|f| f.to_str()) {
            println!("File changed: {}", file_name);

            if let Err(err) = sync_tx.send(file_name.to_owned()).await {
                eprintln!("sync_tx send error: {}", err);
            }
        } else {
            println!("Couldn't extract file name from path: {:?}", path);
        }
    }
}
