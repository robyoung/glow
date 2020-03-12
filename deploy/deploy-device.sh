#!/usr/bin/env bash

set -e

PACKAGE="glow-device"
TRIPPLE="arm-unknown-linux-musleabihf"
TARGET_DIR="./target/${TRIPPLE}/release"
TARGET="${TARGET_DIR}/${PACKAGE}"
SERVICE_NAME="glow.service"
SERVICE_PATH="/etc/systemd/system/${SERVICE_NAME}"

fail() {
  >&2 echo $1
  exit 1
}


[ -n "$BOBNET_GLOW_DEPLOY_TARGET" ] || fail "Deployment target is required"
[ -n "$IFTTT_WEBHOOK_KEY" ] || fail "IFTT webhook key is required"


ssh $BOBNET_GLOW_DEPLOY_TARGET hostname > /dev/null
cross build --package=$PACKAGE --release --target=$TRIPPLE

image=$(docker images --format '{{.Repository}}:{{.Tag}}' rustembedded/cross | grep $TRIPPLE)
docker \
  run --rm -ti \
  -v $(pwd):/usr/src/glow \
  $image \
  /usr/local/arm-linux-musleabihf/bin/strip /usr/src/glow/${TARGET}

md5sum --quiet -c ${TARGET}.md5sum && fail "No change to release binary"
ssh $BOBNET_GLOW_DEPLOY_TARGET sudo systemctl stop $SERVICE_NAME
scp ${TARGET} $BOBNET_GLOW_DEPLOY_TARGET:

cat deploy/glow-device.service | \
  envsubst | \
  ssh $BOBNET_GLOW_DEPLOY_TARGET "cat >/tmp/$SERVICE_NAME"
ssh $BOBNET_GLOW_DEPLOY_TARGET "cmp /tmp/$SERVICE_NAME $SERVICE_PATH || { sudo mv /tmp/$SERVICE_NAME $SERVICE_PATH && sudo systemctl daemon-reload ; }"
ssh $BOBNET_GLOW_DEPLOY_TARGET sudo systemctl start glow.service
md5sum ${TARGET} > ${TARGET}.md5sum
