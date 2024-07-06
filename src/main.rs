use bitckers::app::{App, AppResult};
use bitckers::bitcoin::try_connect_to_node;
use bitckers::config::CmdConfigProvider;
use bitckers::event::{Event, EventHandler};
use bitckers::handler::handle_key_events;
use bitckers::tui::Tui;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::{env, io};

#[tokio::main]
async fn main() -> AppResult<()> {
    let mut app = App::new();

    let backend = CrosstermBackend::new(io::stderr());
    let terminal = Terminal::new(backend)?;
    let events = EventHandler::new(250);
    let mut tui = Tui::new(terminal, events);

    let (args, argv) = argmap::parse(env::args());
    let config_provider = CmdConfigProvider::new(args, argv);

    tui.init()?;

    while app.running {
        tui.draw(&mut app)?;
        try_connect_to_node(config_provider.clone(), app.bitcoin_state.clone())
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
