#!/usr/bin/env bash

set -e

PACKAGE="glow-web"
TARGET_DIR="./target/release"
TARGET="${TARGET_DIR}/${PACKAGE}"
SERVICE_NAME="${PACKAGE}.service"
SERVICE_PATH="/etc/systemd/system/${SERVICE_NAME}"

fail() {
  >&2 echo $1
  exit 1
}

[ -n "$GLOW_WEB_DEPLOY_TARGET" ] || fail "Deployment target is required"

ssh $GLOW_WEB_DEPLOY_TARGET hostname > /dev/null
cargo build --release --package=$PACKAGE
strip $TARGET

md5sum --quiet -c ${TARGET}.md5sum && fail "No change to release binary"

ssh $GLOW_WEB_DEPLOY_TARGET "mkdir -p /var/lib/${PACKAGE} && chown glow:glow /var/lib/${PACKAGE}"
ssh $GLOW_WEB_DEPLOY_TARGET sudo systemctl stop ${SERVICE_NAME}
scp ${TARGET} $GLOW_WEB_DEPLOY_TARGET:/usr/local/bin/${PACKAGE}
ssh $GLOW_WEB_DEPLOY_TARGET "chmod a+x /usr/local/bin/${PACKAGE}"

cat deploy/glow-web.service | \
  envsubst | \
  ssh $GLOW_WEB_DEPLOY_TARGET "cat > /tmp/${SERVICE_NAME}"
ssh $GLOW_WEB_DEPLOY_TARGET "cmp /tmp/${SERVICE_NAME} ${SERVICE_PATH} || { sudo mv /tmp/${SERVICE_NAME} ${SERVICE_PATH} && sudo systemctl daemon-reload ; }"
ssh $GLOW_WEB_DEPLOY_TARGET sudo systemctl start ${SERVICE_NAME}
md5sum ${TARGET} > ${TARGET}.md5sum
