#!/usr/bin/env bash

set -e

TRIPPLE="arm-unknown-linux-musleabihf"
TARGET_DIR="./target/${TRIPPLE}/release"
TARGET="${TARGET_DIR}/glow-device"

fail() {
  >&2 echo $1
  exit 1
}


[ -n "$BOBNET_GLOW_DEPLOY_TARGET" ] || fail "Deployment target is required"
[ -n "$IFTTT_WEBHOOK_KEY" ] || fail "IFTT webhook key is required"


ssh $BOBNET_GLOW_DEPLOY_TARGET hostname > /dev/null
cross build --release --target=$TRIPPLE

image=$(docker images --format '{{.Repository}}:{{.Tag}}' rustembedded/cross | grep $TRIPPLE)
docker \
  run --rm -ti \
  -v $(pwd):/usr/src/glow \
  $image \
  /usr/local/arm-linux-musleabihf/bin/strip /usr/src/glow/${TARGET}

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
