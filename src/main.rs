use bitckers::app::{App, AppResult};
use bitckers::bitcoin::{get_blocks, BitcoinState};
use bitckers::event::{Event, EventHandler};
use bitckers::handler::handle_key_events;
use bitckers::tui::Tui;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() -> AppResult<()> {
    let mut app = App::new();
    let bitcoin_state = Arc::new(Mutex::new(BitcoinState::new()));

    let writable_bitcoin_state = bitcoin_state.clone();
    tokio::spawn(async move {
        get_blocks(writable_bitcoin_state).await;
    });

    let backend = CrosstermBackend::new(io::stderr());
    let terminal = Terminal::new(backend)?;
    let events = EventHandler::new(250);
    let mut tui = Tui::new(terminal, events);

    tui.init()?;

    while app.running {
        let readable_bitcoin_state = bitcoin_state.clone();
        tui.draw(&mut app)?;
        match tui.events.next().await? {
            Event::Tick => app.tick(readable_bitcoin_state),
            Event::Key(key_event) => handle_key_events(key_event, &mut app)?,
            Event::Mouse(_) => {}
            Event::Resize(_, _) => {}
        }
    }

    tui.exit()?;
    Ok(())
}
