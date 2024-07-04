use bitckers::app::{App, AppResult};
use bitckers::bitcoin::{wait_for_blocks, EBitcoinNodeStatus};
use bitckers::event::{Event, EventHandler};
use bitckers::handler::handle_key_events;
use bitckers::tui::Tui;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::{io, time};

#[tokio::main]
async fn main() -> AppResult<()> {
    let mut app = App::new();

    let backend = CrosstermBackend::new(io::stderr());
    let terminal = Terminal::new(backend)?;
    let events = EventHandler::new(250);
    let mut tui = Tui::new(terminal, events);

    tui.init()?;
    tui.draw(&mut app)?;

    while app.running {
        try_connect_to_node(&mut app).await;
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

async fn try_connect_to_node(app: &mut App) {
    let mut unlocked_bitcoin_state = app.bitcoin_state.lock().unwrap();

    match unlocked_bitcoin_state.status {
        EBitcoinNodeStatus::Connecting | EBitcoinNodeStatus::Offline => {
            match unlocked_bitcoin_state.connect_rpc().await {
                Ok(rpc) => match unlocked_bitcoin_state.connect_zmq().await {
                    Ok(socket) => {
                        let wait_blocks_state = app.bitcoin_state.clone();
                        tokio::spawn(async move {
                            wait_for_blocks(socket, wait_blocks_state).await;
                        });

                        let try_connection_state = app.bitcoin_state.clone();
                        tokio::spawn(async move {
                            let mut interval =
                                tokio::time::interval(time::Duration::from_millis(10000));

                            loop {
                                let is_connected = match try_connection_state.lock().unwrap().status
                                {
                                    EBitcoinNodeStatus::Connecting
                                    | EBitcoinNodeStatus::Offline => false,
                                    _ => true,
                                };

                                if is_connected {
                                    {
                                        let _ = try_connection_state
                                            .lock()
                                            .unwrap()
                                            .check_rpc_connection(&rpc);
                                    }
                                    interval.tick().await;
                                } else {
                                    break;
                                }
                            }
                        });
                    }
                    _ => (),
                },
                _ => (),
            }
        }
        _ => (),
    }
}
