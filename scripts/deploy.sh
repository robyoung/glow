#!/usr/bin/env bash

set -e

TARGET_DIR="./target/arm-unknown-linux-gnueabihf/release"
TARGET="${TARGET_DIR}/glow"

fail() {
  >&2 echo $1
  exit 1
}


[ -n "$BOBNET_GLOW_DEPLOY_TARGET" ] || fail "Deployment target is required"
[ -n "$IFTTT_WEBHOOK_KEY" ] || fail "IFTT webhook key is required"


ssh $BOBNET_GLOW_DEPLOY_TARGET hostname > /dev/null
rustup run nightly cargo build --quiet --release --target=arm-unknown-linux-gnueabihf
md5sum --quiet -c ${TARGET}.md5sum && {
  >&2 echo "No change to release binary"
  exit 1
}
ssh $BOBNET_GLOW_DEPLOY_TARGET sudo systemctl stop glow.service
scp ${TARGET} $BOBNET_GLOW_DEPLOY_TARGET:

cat glow.service | envsubst | ssh $BOBNET_GLOW_DEPLOY_TARGET "cat >/tmp/glow.service"
ssh $BOBNET_GLOW_DEPLOY_TARGET "cmp /tmp/glow.service /etc/systemd/system/glow.service || { sudo mv /tmp/glow.service /etc/systemd/system/glow.service && sudo systemctl daemon-reload ; }"
ssh $BOBNET_GLOW_DEPLOY_TARGET sudo systemctl start glow.service
md5sum ${TARGET} > ${TARGET}.md5sum
