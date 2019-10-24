#!/usr/bin/env bash

set -e

fail() {
  >&2 echo $1
  exit 1
}


[ -n "$BOBNET_GLOW_DEPLOY_TARGET" ] || fail "Deployment target is required"
[ -n "$IFTTT_WEBHOOK_KEY" ] || fail "IFTT webhook key is required"


ssh $BOBNET_GLOW_DEPLOY_TARGET hostname
rustup run nightly cargo build --release --target=arm-unknown-linux-gnueabihf
scp ./target/arm-unknown-linux-gnueabihf/release/glow $BOBNET_GLOW_DEPLOY_TARGET:

cat glow.service | envsubst | ssh $BOBNET_GLOW_DEPLOY_TARGET "cat >/tmp/glow.service"
ssh $BOBNET_GLOW_DEPLOY_TARGET "cmp /tmp/glow.service /etc/systemd/system/glow.service || { sudo mv /tmp/glow.service /etc/systemd/system/glow.service && sudo systemctl daemon-reload ; }"

