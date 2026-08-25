use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use evdev::{AbsoluteAxisCode, Device, EventType, InputEvent, KeyCode};
use std::path::PathBuf;
use tokio::sync::mpsc;

use crate::app::AppThread;
use crate::config::TouchSettings;
use crate::event::Event;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TouchMap {
    pub min_x: i32,
    pub max_x: i32,
    pub min_y: i32,
    pub max_y: i32,
    pub swap_xy: bool,
    pub invert_x: bool,
    pub invert_y: bool,
}

impl TouchMap {
    pub fn cell(self, raw_x: i32, raw_y: i32, cols: u16, rows: u16) -> (u16, u16) {
        let mut nx = normalize(raw_x, self.min_x, self.max_x);
        let mut ny = normalize(raw_y, self.min_y, self.max_y);
        if self.swap_xy {
            std::mem::swap(&mut nx, &mut ny);
        }
        if self.invert_x {
            nx = 1.0 - nx;
        }
        if self.invert_y {
            ny = 1.0 - ny;
        }
        let col =
            ((nx * f64::from(cols.saturating_sub(1))).round() as u16).min(cols.saturating_sub(1));
        let row =
            ((ny * f64::from(rows.saturating_sub(1))).round() as u16).min(rows.saturating_sub(1));
        (col, row)
    }
}

fn normalize(value: i32, min: i32, max: i32) -> f64 {
    if max <= min {
        return 0.5;
    }
    ((value.clamp(min, max) - min) as f64) / ((max - min) as f64)
}

pub fn spawn_touch_listener(thread: AppThread, settings: TouchSettings) {
    if !settings.enabled {
        return;
    }
    thread.tracker.spawn(async move {
        tokio::select! {
            () = thread.token.cancelled() => {}
            () = run_touch(thread.sender, settings) => {}
        }
    });
}

async fn run_touch(sender: mpsc::UnboundedSender<Event>, settings: TouchSettings) {
    let Some((path, device, map)) = open_touch_device(&settings) else {
        return;
    };
    let _ = path;
    let Ok(mut stream) = device.into_event_stream() else {
        return;
    };

    let mut raw_x = 0;
    let mut raw_y = 0;
    let mut pressed = false;
    let mut was_pressed = false;

    loop {
        let event = match stream.next_event().await {
            Ok(event) => event,
            Err(_) => break,
        };
        apply_event(event, &mut raw_x, &mut raw_y, &mut pressed);
        if pressed && !was_pressed {
            let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
            let (column, row) = map.cell(raw_x, raw_y, cols, rows);
            let _ = std::fs::write(
                "/tmp/btcmon-touch.log",
                format!("raw={raw_x},{raw_y} cell={column},{row} size={cols}x{rows}\n"),
            );
            let _ = sender.send(Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row,
                modifiers: KeyModifiers::NONE,
            }));
        }
        was_pressed = pressed;
    }
}

fn apply_event(event: InputEvent, raw_x: &mut i32, raw_y: &mut i32, pressed: &mut bool) {
    match event.event_type() {
        EventType::ABSOLUTE => {
            if event.code() == AbsoluteAxisCode::ABS_X.0 {
                *raw_x = event.value();
            } else if event.code() == AbsoluteAxisCode::ABS_Y.0 {
                *raw_y = event.value();
            } else if event.code() == AbsoluteAxisCode::ABS_PRESSURE.0 {
                *pressed = event.value() > 0;
            }
        }
        EventType::KEY if event.code() == KeyCode::BTN_TOUCH.0 => {
            *pressed = event.value() > 0;
        }
        _ => {}
    }
}

fn open_touch_device(settings: &TouchSettings) -> Option<(PathBuf, Device, TouchMap)> {
    if !settings.device.trim().is_empty() {
        return load_device(PathBuf::from(settings.device.trim()), settings);
    }
    evdev::enumerate().find_map(|(path, device)| {
        let name = device.name().unwrap_or("");
        let looks_like_touch = name.contains("ADS7846")
            || name.contains("Touchscreen")
            || name.contains("Touch")
            || device.supported_absolute_axes().is_some_and(|axes| {
                axes.contains(AbsoluteAxisCode::ABS_X) && axes.contains(AbsoluteAxisCode::ABS_Y)
            });
        if looks_like_touch {
            load_device_from(path, device, settings)
        } else {
            None
        }
    })
}

fn load_device(path: PathBuf, settings: &TouchSettings) -> Option<(PathBuf, Device, TouchMap)> {
    let device = Device::open(&path).ok()?;
    load_device_from(path, device, settings)
}

fn load_device_from(
    path: PathBuf,
    device: Device,
    settings: &TouchSettings,
) -> Option<(PathBuf, Device, TouchMap)> {
    let mut min_x = 0;
    let mut max_x = 4095;
    let mut min_y = 0;
    let mut max_y = 4095;
    if let Ok(info) = device.get_absinfo() {
        for (axis, abs) in info {
            if axis == AbsoluteAxisCode::ABS_X {
                min_x = abs.minimum();
                max_x = abs.maximum();
            } else if axis == AbsoluteAxisCode::ABS_Y {
                min_y = abs.minimum();
                max_y = abs.maximum();
            }
        }
    }
    Some((
        path,
        device,
        TouchMap {
            min_x,
            max_x,
            min_y,
            max_y,
            swap_xy: settings.swap_xy,
            invert_x: settings.invert_x,
            invert_y: settings.invert_y,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::TouchMap;

    fn map() -> TouchMap {
        TouchMap {
            min_x: 0,
            max_x: 1000,
            min_y: 0,
            max_y: 1000,
            swap_xy: false,
            invert_x: false,
            invert_y: false,
        }
    }

    #[test]
    fn identity_map_sends_max_raw_to_bottom_right() {
        let touch = map();
        assert_eq!(touch.cell(0, 0, 80, 24), (0, 0));
        assert_eq!(touch.cell(1000, 1000, 80, 24), (79, 23));
        assert_eq!(touch.cell(0, 1000, 80, 24), (0, 23));
        assert_eq!(touch.cell(1000, 0, 80, 24), (79, 0));
    }
}
