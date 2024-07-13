use btcmon::app::{App, AppResult, AppThread};
use btcmon::config;
use btcmon::event::{Event, EventHandler};
use btcmon::node::node::NodeProvider;
use btcmon::node::providers::bitcoin_core::BitcoinCore;
use btcmon::tui::Tui;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::{env, io};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> AppResult<()> {
    let (args, argv) = argmap::parse(env::args());
    let config = config::AppConfig::new(args, argv).unwrap();
    let config_clone = config.clone();

    let (sender, receiver) = mpsc::unbounded_channel();

    let sender_clone = sender.clone();
    let thread = AppThread::new(sender_clone);

    let provider: Box<dyn NodeProvider + Send + 'static> = match config.bitcoin_core.host {
        _ => Box::new(BitcoinCore::new(config_clone))
    };
    
    let mut app = App::new(thread, provider);

    let backend = CrosstermBackend::new(io::stderr());
    let terminal = Terminal::new(backend)?;
    let events = EventHandler::new(
        config.tick_rate.parse::<u64>().unwrap(),
        app.thread.sender.clone(),
        receiver,
    );

    let mut tui = Tui::new(terminal, events);
    tui.init()?;
    tui.draw(&config, &mut app)?;

    app.init_node();

    if config.price.enabled {
        app.init_price();
    }

    while app.running {
        tui.draw(&config, &mut app)?;
        match tui.events.next().await? {
            Event::Tick => app.tick(),
            Event::Key(key_event) => app.handle_key_events(key_event)?,
            Event::Mouse(_) => {}
            Event::Resize(_, _) => {}
            Event::PriceUpdate(state) => app.handle_price_update(state),
        }
    }

    app.thread.tracker.close();
    app.thread.token.cancel();
    app.thread.tracker.wait().await;

    tui.exit()?;
    Ok(())
}
