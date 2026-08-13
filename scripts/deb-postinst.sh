#!/bin/sh
# postinst script for the Cloakwire .deb.
#
# Why this exists: TUN mode on Linux requires the sing-box binary
# itself to hold `cap_net_admin` + `cap_net_raw` (it opens
# `/dev/net/tun` and configures addresses / routes via netlink,
# all of which need admin caps). Without these caps sing-box dies
# the moment you press Connect with "operation not permitted".
#
# The standard fix is `setcap`, which we apply here at install time
# so the user never has to think about it. Both caps are widely
# understood by sing-box / v2ray / wireguard / etc. and don't give
# sing-box anything it wouldn't already need to do its job.
#
# Idempotent: subsequent installs (e.g. `apt upgrade`) re-run this
# and `setcap` quietly no-ops if the caps are already there.
#
# Failure mode: we `|| true` on the setcap line so a missing
# `setcap` (libcap2-bin not installed — extremely rare) doesn't
# break the whole install. The app surfaces a friendly recovery
# error on first start instead.
set -e

SINGBOX_BIN="/usr/bin/sing-box"

if [ -x "$SINGBOX_BIN" ] && command -v setcap >/dev/null 2>&1; then
    setcap cap_net_admin,cap_net_raw=+ep "$SINGBOX_BIN" || true
fi

#DEBHELPER#

exit 0
