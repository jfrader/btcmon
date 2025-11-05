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

## Configuration Options

```toml
tick_rate = 250

[node]
provider = "core_lightning"

[bitcoin_core]
host = "127.0.0.1"
rpc_port = 18443
rpc_user = "polaruser"
rpc_password = "polarpass"
zmq_port = 28334

[core_lightning]
rest_address = "http://127.0.0.1:3010"
rest_rune = "replaceme"

[lnd]
rest_address = "https://127.0.0.1:8080"
macaroon_hex = "replaceme"

[price]
enabled = true
currency = "USD"
big_text = true

[fees]
enabled = true

```

## Screenshot

![btcmon](share/screenshots/btcmon.png?raw=true)

## License

This program is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.

You should have received a copy of the GNU General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
