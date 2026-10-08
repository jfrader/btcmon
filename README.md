# Terminal Bitcoin Monitor

[![ci](https://img.shields.io/github/actions/workflow/status/jfrader/btcmon/rust.yml?branch=master&style=flat&label=ci)](https://github.com/jfrader/btcmon/actions)
[![license](https://img.shields.io/github/license/jfrader/btcmon?style=flat)](./LICENSE)

![btcmon](share/screenshots/demo.gif?raw=true)

Command line monitor for the Bitcoin Network and your Bitcoin and Lightning node.

## Install and run

```sh
cargo install --git https://github.com/jfrader/btcmon
btcmon --config /path/to/config.toml  # default: ~/.btcmon/btcmon.toml
```

Config keys also work as flags, e.g. `--bitcoin_core.rpc_user=user`.

## Touch controls

On a Linux framebuffer console, btcmon reads the digitizer (`ADS7846` / tft35a)
directly. Xterm mouse reporting still works under VNC. The bottom dock provides
large touch targets:

- `<` / `>` selects and pins the previous or next node so it stays on screen.
- Tapping the node name toggles rotation between `AUTO` and `PINNED` (and resumes automatic rotation after a manual selection).
- Tapping `VIEW` opens the Overview, Node, Price, and Fees picker. Only enabled views are shown.

The keyboard equivalents are Left/Right (node), Space or `r` (auto/pinned), `v` (view picker), Tab/Shift-Tab (next/previous view), `1`-`4` (direct view), and `q`/Esc (quit). If the view picker is open, Esc closes it first.

When price is the only enabled source, the dock is hidden so the price keeps the entire screen. Enabling fees in a price-only config adds the touch view picker.

## Configuration

Start from an example: [single node](share/config/example.toml), [multiple nodes](share/config/example-multiple.toml), or [price only](share/config/price-only.toml).

- `[[nodes]]`: optional `name` (shown in the dock) and `provider`: `bitcoin_core` (`host`, `rpc_port`, `rpc_user`, `rpc_password`, `zmq_port`), `core_lightning` (`rest_address`, `rest_rune`), or `lnd` (`rest_address`, `macaroon_hex`).
- `[price]`: `enabled`, `currency`, `big_text`, `variation`, `variation_threshold`. `[fees]`: `enabled`.
- `[touch]`: `enabled`, `device` (empty auto-detects), `swap_xy`, `invert_x`, `invert_y`.
- Top level: `tick_rate` (ms), `node_switch_interval` (s), `streamer_mode` (hides channel capacity).

## Screenshot

![btcmon](share/screenshots/btcmon.png?raw=true)

## License

This program is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.

You should have received a copy of the GNU General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
