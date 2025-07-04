use std::sync::Arc;

use crossterm::event::{Event as CrosstermEvent, KeyEvent, MouseEvent};
use futures::{FutureExt, StreamExt};
use tokio::sync::mpsc;
use tokio::time::Duration;

use crate::{app::AppResult, fees::FeesState, node::NodeState, price::PriceState};

#[derive(Clone)]
pub enum Event {
    Tick,
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
    PriceUpdate(PriceState),
    FeeUpdate(FeesState),
    NodeUpdate(Arc<dyn Fn(NodeState) -> NodeState + Send + Sync>),
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct EventHandler {
    pub sender: mpsc::UnboundedSender<Event>,
    pub receiver: mpsc::UnboundedReceiver<Event>,
    handler: tokio::task::JoinHandle<()>,
}

impl EventHandler {
    pub fn new(
        tick_rate: u64,
        sender: mpsc::UnboundedSender<Event>,
        receiver: mpsc::UnboundedReceiver<Event>,
    ) -> Self {
        let tick_rate = Duration::from_millis(tick_rate);
        let _sender = sender.clone();
        let handler = tokio::spawn(async move {
            let mut reader = crossterm::event::EventStream::new();
            let mut tick = tokio::time::interval(tick_rate);
            loop {
                let tick_delay = tick.tick();
                let crossterm_event = reader.next().fuse();
                tokio::select! {
                  _ = _sender.closed() => {
                    break;
                  }
                  _ = tick_delay => {
                    _sender.send(Event::Tick).unwrap();
                  }
                  Some(Ok(evt)) = crossterm_event => {
                    match evt {
                      CrosstermEvent::Key(key) => {
                        if key.kind == crossterm::event::KeyEventKind::Press {
                          _sender.send(Event::Key(key)).unwrap();
                        }
                      },
                      CrosstermEvent::Mouse(mouse) => {
                        _sender.send(Event::Mouse(mouse)).unwrap();
                      },
                      CrosstermEvent::Resize(x, y) => {
                        _sender.send(Event::Resize(x, y)).unwrap();
                      },
                      CrosstermEvent::FocusLost => {
                      },
                      CrosstermEvent::FocusGained => {
                      },
                      CrosstermEvent::Paste(_) => {
                      },
                    }
                  }
                };
            }
        });
        Self {
            sender,
            receiver,
            handler,
        }
    }

    /// Receive the next event from the handler thread.
    ///
    /// This function will always block the current thread if
    /// there is no data available and it's possible for more data to be sent.
    pub async fn next(&mut self) -> AppResult<Event> {
        let receiver = &mut self.receiver;
        receiver.recv().await.ok_or(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "This is an IO error",
        )))
    }
}
