# Terminal Bitcoin Monitor

![btcmon](share/screenshots/demo.gif?raw=true)

Command line monitor for the Bitcoin Network and your Bitcoin and Lightning node.

## Installation

`git clone https://github.com/jfrader/btcmon.git`

`cd btcmon`

`cargo install --path .`

## Usage

```sh
btcmon --bitcoin_core.rpc_user="user" --bitcoin_core.rpc_password="password"
```
or

```sh
btcmon --config /path/to/config # default /etc/btcmon/btcmon.toml and ~/.btcmon/btcmon.toml
```

See the [Example config.toml](share/config/example.toml) file

## Touch controls

On displays with mouse reporting enabled, the bottom dock provides large touch targets:

- `<` / `>` selects and pins the previous or next node so it stays on screen.
- Tapping the node name toggles rotation between `AUTO` and `PINNED` (and resumes automatic rotation after a manual selection).
- Tapping `VIEW` opens the Overview, Node, Price, and Fees picker. Only enabled views are shown.

The keyboard equivalents are Left/Right (node), Space or `r` (auto/pinned), `v` (view picker), Tab/Shift-Tab (next/previous view), `1`-`4` (direct view), and `q`/Esc (quit). If the view picker is open, Esc closes it first.

When price is the only enabled source, the dock is hidden so the price keeps the entire screen. Enabling fees in a price-only config adds the touch view picker.

## Configuration options

```toml
tick_rate = 250
node_switch_interval = 5

[[nodes]]
name = "Bitcoin Core"
provider = "bitcoin_core"
[nodes.bitcoin_core]
host = "127.0.0.1"
rpc_port = 8332
rpc_user = "user"
rpc_password = "password"
zmq_port = 28334

[[nodes]]
name = "Lightning"
provider = "core_lightning"
[nodes.core_lightning]
rest_address = "http://127.0.0.1:3010"
rest_rune = "replaceme"

[price]
enabled = true
currency = "USD"
big_text = true
variation = "minute"
variation_threshold = 0.0

[fees]
enabled = true
```

Each `[[nodes]]` entry can use `bitcoin_core`, `core_lightning`, or `lnd`. The optional `name` is used in the touch dock. See [the multiple-node example](share/config/example-multiple.toml) and [the price-only example](share/config/price-only.toml).

## Screenshot

![btcmon](share/screenshots/btcmon.png?raw=true)

## License

This program is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.

You should have received a copy of the GNU General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
