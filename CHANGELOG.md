# Changelog

## Unreleased

### Fixes

- Core Lightning channel row counts connected `CHANNELD_NORMAL` peers as up, and disconnected or closing ones as down.

### Improvements

- Price-only draws a multi-row line chart with a moving `*` for the latest tick, instead of a one-line bar strip and H/L footer.
- Price and fee panels keep the last good value when a refresh fails and show `STALE` or `ERR` instead of going blank.
- Node views show glanceable facts: Core peers/mempool/disk/sync, compact Lightning channel rows. Price-only adds a sparkline and session high/low.
- Touch on the Pi framebuffer consoles now drives the dock (ADS7846 / tft35a).
- On a short screen, tapping the left or right half of the node panel switches nodes without pinning. Tap AUTO/PINNED on the dock to lock a node.
