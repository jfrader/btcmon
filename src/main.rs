use bitckers::app::{App, AppResult};
use bitckers::bitcoin::wait_for_blocks;
use bitckers::event::{Event, EventHandler};
use bitckers::handler::handle_key_events;
use bitckers::tui::Tui;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;

#[tokio::main]
async fn main() -> AppResult<()> {
    let mut app = App::new();

    let backend = CrosstermBackend::new(io::stderr());
    let terminal = Terminal::new(backend)?;
    let events = EventHandler::new(250);
    let mut tui = Tui::new(terminal, events);

    tui.init()?;
    tui.draw(&mut app)?;

    {
        match app.bitcoin_state.lock().unwrap().connect() {
            Ok(_) => {
                let writable_bitcoin_state = app.bitcoin_state.clone();
                tokio::spawn(async move {
                    wait_for_blocks(writable_bitcoin_state).await;
                });
            }
            Err(_) => (),
        };
    }

    while app.running {
        tui.draw(&mut app)?;
        match tui.events.next().await? {
            Event::Tick => app.tick(),
            Event::Key(key_event) => handle_key_events(key_event, &mut app)?,
            Event::Mouse(_) => {}
            Event::Resize(_, _) => {}
        }
    }

    tui.exit()?;
    Ok(())
}
