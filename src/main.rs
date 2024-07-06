use btcmon::app::{App, AppResult};
use btcmon::bitcoin::try_connect_to_node;
use btcmon::config;
use btcmon::event::{Event, EventHandler};
use btcmon::handler::handle_key_events;
use btcmon::tui::Tui;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::{env, io};

#[tokio::main]
async fn main() -> AppResult<()> {
    let (args, argv) = argmap::parse(env::args());
    let config = config::Settings::new(args, argv).unwrap();

    let mut app = App::new(config.tick_rate.parse::<u16>().unwrap());

    let backend = CrosstermBackend::new(io::stderr());
    let terminal = Terminal::new(backend)?;
    let events = EventHandler::new(config.tick_rate.parse::<u64>().unwrap());
    let mut tui = Tui::new(terminal, events);

    tui.init()?;

    while app.running {
        tui.draw(&mut app)?;
        try_connect_to_node(config.clone(), app.bitcoin_state.clone())
            .await
            .unwrap_or_else(|_| ());
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
