# Terminal Bitcoin Monitor

**!Work in progres!**

Command line monitor for the Bitcoin Network and your Bitcoin and Lightning node.

## Screenshots

![1](share/screenshots/1.jpg?raw=true)

![2](share/screenshots/2.jpg?raw=true)

![3](share/screenshots/3.jpg?raw=true)

## Installation

`git clone https://github.com/jfrader/btcmon.git`

`cd btcmon`

`cargo install --path .`

## Usage

```sh
btcmon --bitcoin_core.rpc_user="user" --bitcoin_core.rpc_password="password"

or

```sh
btcmon --config /path/to/config # default /etc/btcmon/btcmon.toml and ~/.btcmon/btcmon.toml
```

[Example config.toml](share/config/example.toml)

## License

This program is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.

You should have received a copy of the GNU General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
