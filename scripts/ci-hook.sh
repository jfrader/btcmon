#!/bin/sh
# Called from CI on a self-hosted runner. Does nothing unless the operator
# pointed BTCMON_CI_HOOK at a machine-local command.
set -eu

if [ -z "${BTCMON_CI_HOOK:-}" ]; then
  echo "ci-hook: BTCMON_CI_HOOK unset, nothing to do"
  exit 0
fi

exec "$BTCMON_CI_HOOK"
