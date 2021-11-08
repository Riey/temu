mod event;
mod render;
mod term;

use std::sync::Arc;

fn main() {
    env_logger::init();
    let (event_tx, event_rx) = self::event::channel();
    let shared = Arc::new(self::term::SharedTerminal::new());

    let tx_inner = event_tx.clone();
    let shared_inner = shared.clone();

    std::thread::spawn(move || {
        term::run(tx_inner, shared_inner);
    });

    render::run(event_tx, event_rx, shared);
}
