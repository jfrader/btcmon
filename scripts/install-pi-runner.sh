#!/bin/sh
# Install the machine-local CI hook, and optionally a GitHub runner.
#   sudo BTCMON_ROLE=pi1 BTCMON_RUNNER_TOKEN=... ./scripts/install-pi-runner.sh
#   sudo BTCMON_ROLE=pi2 BTCMON_SKIP_RUNNER=1 ./scripts/install-pi-runner.sh
set -eu

ROLE="${BTCMON_ROLE:-}"
TOKEN="${BTCMON_RUNNER_TOKEN:-}"
REPO_URL="${BTCMON_REPO_URL:-https://github.com/jfrader/btcmon}"
RUNNER_VERSION="${BTCMON_RUNNER_VERSION:-2.336.0}"
USER_NAME="btcmon-actions"
HOME_DIR="/var/lib/btcmon-actions"
OPT_DIR="/opt/btcmon-actions"

if [ "$(id -u)" -ne 0 ]; then
  echo "run as root (sudo $0)" >&2
  exit 1
fi

case "$ROLE" in
  pi1)
    NAME="btcmon-pi1"
    LABELS="linux,ARM64"
    INSTALL_RUST=1
    ;;
  pi2)
    NAME="btcmon-pi2"
    LABELS="linux,ARM64"
    INSTALL_RUST=0
    ;;
  *)
    echo "BTCMON_ROLE must be pi1 or pi2" >&2
    exit 1
    ;;
esac

if [ "${BTCMON_SKIP_RUNNER:-0}" != "1" ] && [ -z "$TOKEN" ]; then
  echo "BTCMON_RUNNER_TOKEN is required" >&2
  exit 1
fi

apt-get update
apt-get install -y libzmq3-dev pkg-config file curl ca-certificates

if ! id "$USER_NAME" >/dev/null 2>&1; then
  useradd --system --home-dir "$HOME_DIR" --create-home --shell /usr/sbin/nologin "$USER_NAME"
fi
install -d -o "$USER_NAME" -g "$USER_NAME" -m 0750 "$HOME_DIR" "$HOME_DIR/incoming" "$OPT_DIR"
install -d -o root -g root -m 0755 /etc/btcmon

if [ "$INSTALL_RUST" -eq 1 ] && [ ! -x "$HOME_DIR/.cargo/bin/rustc" ]; then
  sudo -u "$USER_NAME" -H env HOME="$HOME_DIR" bash -lc \
    'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable'
fi

SKIP_RUNNER="${BTCMON_SKIP_RUNNER:-0}"
if [ "$SKIP_RUNNER" != "1" ]; then
  if [ ! -x "$OPT_DIR/bin/Runner.Listener" ]; then
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    curl -fsSL -o "$tmp/runner.tgz" \
      "https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-linux-arm64-${RUNNER_VERSION}.tar.gz"
    tar -xzf "$tmp/runner.tgz" -C "$OPT_DIR"
    chown -R "$USER_NAME:$USER_NAME" "$OPT_DIR"
  fi

  if [ ! -f "$OPT_DIR/.runner" ]; then
    sudo -u "$USER_NAME" -H env HOME="$HOME_DIR" \
      "$OPT_DIR/config.sh" \
      --unattended \
      --replace \
      --url "$REPO_URL" \
      --token "$TOKEN" \
      --name "$NAME" \
      --labels "$LABELS"
  fi
fi

install -o root -g root -m 0755 /dev/stdin /usr/local/sbin/btcmon-deploy <<'EOF'
#!/bin/sh
set -eu

INCOMING="${BTCMON_INCOMING:-/var/lib/btcmon-actions/incoming/btcmon}"
DEST="${BTCMON_DEST:-/home/snead/.cargo/bin/btcmon}"
FLAG="${BTCMON_AUTODEPLOY_FLAG:-/etc/btcmon/auto-deploy.enabled}"

if [ ! -f "$FLAG" ]; then
  echo "btcmon-deploy: skipped (missing $FLAG)"
  exit 0
fi

if [ ! -f "$INCOMING" ]; then
  echo "btcmon-deploy: missing incoming binary: $INCOMING" >&2
  exit 1
fi

if ! file -b "$INCOMING" | grep -q "ARM aarch64"; then
  echo "btcmon-deploy: refusing non-aarch64 binary: $(file -b "$INCOMING")" >&2
  exit 1
fi

install -o snead -g snead -m 0755 "$INCOMING" "$DEST"
systemctl restart getty@tty1.service
sleep 2

if ! pgrep -x -u snead btcmon >/dev/null; then
  echo "btcmon-deploy: btcmon did not come back after getty restart" >&2
  exit 1
fi

echo "btcmon-deploy: installed $DEST and restarted tty1"
EOF

install -o root -g root -m 0755 /dev/stdin /usr/local/sbin/btcmon-ci-hook <<'EOF'
#!/bin/sh
set -eu
incoming=/var/lib/btcmon-actions/incoming/btcmon
src="${GITHUB_WORKSPACE:-}/target/release/btcmon"
if [ -f "$src" ]; then
  install -m 0755 "$src" "$incoming"
fi
sudo /usr/local/sbin/btcmon-deploy
if [ -f /etc/btcmon/peers ] && [ -f "$incoming" ]; then
  while read -r peer; do
    [ -n "$peer" ] || continue
    case "$peer" in \#*) continue ;; esac
    scp -o BatchMode=yes -o IdentitiesOnly=yes "$incoming" "$peer:/tmp/btcmon-incoming"
    ssh -o BatchMode=yes -o IdentitiesOnly=yes "$peer" \
      "sudo install -m 0755 /tmp/btcmon-incoming /var/lib/btcmon-actions/incoming/btcmon && sudo /usr/local/sbin/btcmon-deploy && rm -f /tmp/btcmon-incoming"
  done < /etc/btcmon/peers
fi
EOF

cat >/etc/sudoers.d/btcmon-actions <<'EOF'
btcmon-actions ALL=(root) NOPASSWD: /usr/local/sbin/btcmon-deploy
EOF
chmod 0440 /etc/sudoers.d/btcmon-actions

cat >/etc/systemd/system/btcmon-actions.service <<EOF
[Unit]
Description=GitHub Actions runner for jfrader/btcmon
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=${OPT_DIR}/bin/runsvc.sh
User=${USER_NAME}
Group=${USER_NAME}
WorkingDirectory=${OPT_DIR}
Environment=HOME=${HOME_DIR}
Environment=PATH=${HOME_DIR}/.cargo/bin:/usr/local/bin:/usr/bin:/bin
Environment=BTCMON_CI_HOOK=/usr/local/sbin/btcmon-ci-hook
KillMode=process
KillSignal=SIGTERM
TimeoutStopSec=5min
Restart=always
RestartSec=5
UMask=0077

[Install]
WantedBy=multi-user.target
EOF

if [ "$SKIP_RUNNER" != "1" ]; then
  systemctl daemon-reload
  systemctl enable --now btcmon-actions.service
fi

echo "installed $NAME (auto-deploy still off until /etc/btcmon/auto-deploy.enabled exists)"
