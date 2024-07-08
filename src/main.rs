use btcmon::app::{App, AppResult};
use btcmon::config;
use btcmon::event::{Event, EventHandler};
use btcmon::handler::handle_key_events;
use btcmon::tui::Tui;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::{env, io};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> AppResult<()> {
    let (args, argv) = argmap::parse(env::args());
    let config = config::Settings::new(args, argv).unwrap();

    let (sender, receiver) = mpsc::unbounded_channel();

    let mut app = App::new(sender);
    let backend = CrosstermBackend::new(io::stderr());
    let terminal = Terminal::new(backend)?;
    let events = EventHandler::new(
        config.tick_rate.parse::<u64>().unwrap(),
        app.sender.clone(),
        receiver,
    );
    let mut tui = Tui::new(terminal, events);

    tui.init()?;

    if config.price.enabled {
        app.init_price();
    }

    app.init_bitcoin();

    while app.running {
        tui.draw(&config, &mut app)?;
        match tui.events.next().await? {
            Event::Tick => app.tick(),
            Event::Key(key_event) => handle_key_events(key_event, &mut app)?,
            Event::Mouse(_) => {}
            Event::Resize(_, _) => {}
            Event::BitcoinCoreLostConnection => app.init_bitcoin(),
            Event::PriceUpdate(state) => app.handle_price_update(state),
        }
    }

    app.thread_tracker.close();
    app.thread_token.cancel();
    app.thread_tracker.wait().await;

    tui.exit()?;
    Ok(())
}
