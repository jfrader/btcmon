use btcmon::app::{App, AppResult};
use btcmon::bitcoin::try_connect_to_node;
use btcmon::config;
use btcmon::event::{Event, EventHandler};
use btcmon::handler::handle_key_events;
use btcmon::tui::Tui;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::{env, io};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

#[tokio::main]
async fn main() -> AppResult<()> {
    let (args, argv) = argmap::parse(env::args());
    let config = config::Settings::new(args, argv).unwrap();

    let mut app = App::new();
    let backend = CrosstermBackend::new(io::stderr());
    let terminal = Terminal::new(backend)?;
    let events = EventHandler::new(config.tick_rate.parse::<u64>().unwrap());
    let mut tui = Tui::new(terminal, events);

    tui.init()?;
    let connect_node_tracker = TaskTracker::new();
    let connect_node_token = CancellationToken::new();
    let connect_node_tracker_clone = connect_node_tracker.clone();

    try_connect_to_node(
        config.clone(),
        &mut app,
        connect_node_tracker.clone(),
        connect_node_token.clone(),
    );

    while app.running {
        tui.draw(&config, &mut app)?;
        match tui.events.next().await? {
            Event::Tick => app.tick(),
            Event::Key(key_event) => handle_key_events(key_event, &mut app)?,
            Event::Mouse(_) => {}
            Event::Resize(_, _) => {}
        }
    }

    connect_node_tracker.close();
    connect_node_token.cancel();
    connect_node_tracker_clone.wait().await;

    tui.exit()?;
    Ok(())
}
