// main.rs

use btcmon::app::{App, AppResult, AppThread};
use btcmon::config;
use btcmon::event::{Event, EventHandler};
use btcmon::node::providers::bitcoin_core::{BitcoinCore, BitcoinCoreWidget};
use btcmon::node::providers::core_lightning::{CoreLightning, CoreLightningWidget};
use btcmon::node::providers::lnd::{LndNode, LndWidget};
use btcmon::node::NodeProvider;
use btcmon::tui::Tui;
use btcmon::widget::{DefaultWidgetState, DynamicNodeStatefulWidget, DynamicState};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::{env, io};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> AppResult<()> {
    let (args, argv) = argmap::parse(env::args());
    let config = config::AppConfig::new(args, argv).unwrap();

    let (sender, receiver) = mpsc::unbounded_channel();

    let sender_clone = sender.clone();
    let thread = AppThread::new(sender_clone);

    let (provider, widget, widget_state): (
        Box<dyn NodeProvider + Send + 'static>,
        Box<dyn DynamicNodeStatefulWidget>,
        Box<dyn DynamicState>,
    ) = match config.node.provider.as_str() {
        "bitcoin_core" => (
            Box::new(BitcoinCore::new(&config)),
            Box::new(BitcoinCoreWidget),
            Box::new(DefaultWidgetState),
        ),
        "core_lightning" => (
            Box::new(CoreLightning::new(&config)),
            Box::new(CoreLightningWidget),
            Box::new(DefaultWidgetState),
        ),
        "lnd" => (
            Box::new(LndNode::new(&config)),
            Box::new(LndWidget),
            Box::new(DefaultWidgetState),
        ),
        other => {
            eprintln!(
                "Unknown node provider: '{}'. Expected one of: bitcoin_core, core_lightning, lnd",
                other
            );
            std::process::exit(1);
        }
    };

    let mut app = App::new(thread, widget, widget_state);

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

    app.init_node(provider);

    if config.price.enabled {
        app.init_price();
    }

    if config.fees.enabled {
        app.init_fees();
    }

    while app.running {
        tui.draw(&config, &mut app)?;
        match tui.events.next().await? {
            Event::Tick => app.tick(),
            Event::Key(key_event) => app.handle_key_events(key_event)?,
            Event::Mouse(_) => {}
            Event::Resize(_, _) => {}
            Event::PriceUpdate(state) => app.handle_price_update(state),
            Event::FeeUpdate(state) => app.handle_fee_update(state),
            Event::NodeUpdate(update_fn) => {
                app.handle_node_update(update_fn.as_ref());
            }
        }
    }

    app.thread.tracker.close();
    app.thread.token.cancel();
    app.thread.tracker.wait().await;

    tui.exit()?;
    Ok(())
}
