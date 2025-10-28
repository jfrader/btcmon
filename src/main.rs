use btcmon::app::{App, AppResult, AppThread};
use btcmon::config;
use btcmon::event::{Event, EventHandler};
use btcmon::node::providers::bitcoin_core::{
    BitcoinCore, BitcoinCoreWidget, BitcoinCoreWidgetState,
};
use btcmon::node::providers::core_lightning::{
    CoreLightning, CoreLightningWidget, CoreLightningWidgetState,
};
use btcmon::node::providers::lnd::{LndNode, LndWidget, LndWidgetState};
use btcmon::node::NodeProvider;
use btcmon::tui::Tui;
use btcmon::widget::{DynamicNodeStatefulWidget, DynamicState};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::env;
use std::io;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> AppResult<()> {
    let (args, argv) = argmap::parse(env::args());
    let config = config::AppConfig::new(args, argv).unwrap();

    let (sender, receiver) = mpsc::unbounded_channel();

    let sender_clone = sender.clone();
    let thread = AppThread::new(sender_clone);

    let mut providers: Vec<Box<dyn NodeProvider + Send + 'static>> = vec![];
    let mut widgets: Vec<Box<dyn DynamicNodeStatefulWidget>> = vec![];
    let mut widget_states: Vec<Box<dyn DynamicState>> = vec![];

    // Use nodes from config.nodes if present, otherwise use single node configuration
    if !config.nodes.is_empty() {
        for node in &config.nodes {
            match node.provider.as_str() {
                "bitcoin_core" => {
                    if let Some(settings) = &node.bitcoin_core {
                        if !settings.host.is_empty() {
                            providers.push(Box::new(BitcoinCore::new(settings)));
                            widgets.push(Box::new(BitcoinCoreWidget));
                            widget_states.push(Box::new(BitcoinCoreWidgetState::default()));
                        }
                    }
                }
                "core_lightning" => {
                    if let Some(settings) = &node.core_lightning {
                        if !settings.rest_address.is_empty() {
                            providers.push(Box::new(CoreLightning::new(settings)));
                            widgets.push(Box::new(CoreLightningWidget));
                            widget_states.push(Box::new(CoreLightningWidgetState::default()));
                        }
                    }
                }
                "lnd" => {
                    if let Some(settings) = &node.lnd {
                        if !settings.rest_address.is_empty() {
                            providers.push(Box::new(LndNode::new(settings)));
                            widgets.push(Box::new(LndWidget));
                            widget_states.push(Box::new(LndWidgetState::default()));
                        }
                    }
                }
                other => {
                    eprintln!("Unknown node provider: '{}'.", other);
                }
            }
        }
    } else {
        // Use single node configuration, prioritizing lnd
        if !config.lnd.rest_address.is_empty() {
            providers.push(Box::new(LndNode::new(&config.lnd)));
            widgets.push(Box::new(LndWidget));
            widget_states.push(Box::new(LndWidgetState::default()));
        } else if !config.core_lightning.rest_address.is_empty() {
            providers.push(Box::new(CoreLightning::new(&config.core_lightning)));
            widgets.push(Box::new(CoreLightningWidget));
            widget_states.push(Box::new(CoreLightningWidgetState::default()));
        } else if !config.bitcoin_core.host.is_empty() {
            providers.push(Box::new(BitcoinCore::new(&config.bitcoin_core)));
            widgets.push(Box::new(BitcoinCoreWidget));
            widget_states.push(Box::new(BitcoinCoreWidgetState::default()));
        } else {
            eprintln!("No nodes or single node configuration found.");
            std::process::exit(1);
        }
    }

    if providers.is_empty() {
        eprintln!("No valid nodes configured.");
        std::process::exit(1);
    }

    let mut app = App::new(thread, widgets, widget_states, config.clone());

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

    // Initialize all nodes
    for (i, provider) in providers.into_iter().enumerate() {
        app.nodes[i].init(provider, i);
    }

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
            Event::Mouse(mouse_event) => app.handle_mouse_events(mouse_event)?,
            Event::Resize(_, _) => {}
            Event::PriceUpdate(state) => app.handle_price_update(state),
            Event::FeeUpdate(state) => app.handle_fee_update(state),
            Event::NodeUpdate(index, update_fn) => {
                app.handle_node_update(index, update_fn.as_ref());
            }
        }
    }

    app.thread.tracker.close();
    app.thread.token.cancel();
    app.thread.tracker.wait().await;

    tui.exit()?;
    Ok(())
}
