#!/usr/bin/env bash

set -e

help() {
  >&2 cat <<EOF
Usage: ./deploy/deploy.sh [web|device|both]

  Deploy script for glow-web and glow-device.
EOF
}

fail() {
  >&2 echo -e "\033[1;31m${1}\033[0m"
  exit 1
}

debug() {
  [ -z "${DEBUG}" ] || { >&2 echo -e "\033[1;36m$1\033[0m"; }
}

build-glow-device() {
  cross build --package=$PACKAGE --release --target=$TRIPPLE
  local image=$(docker images --format '{{.Repository}}:{{.Tag}}' rustembedded/cross | grep $TRIPPLE)
  docker \
    run --rm -ti \
    -v $(pwd):/usr/src/glow \
    $image \
    /usr/local/arm-linux-musleabihf/bin/strip /usr/src/glow/${TARGET}
}

build-glow-web() {
  cargo build --release --package=$PACKAGE
}

check-host() {
  ssh $DEPLOY_TARGET hostname > /dev/null
  debug "Check host $DEPLOY_TARGET: OK"
}

prepare-glow-device() {
  cat /dev/null
}

prepare-glow-web() {
  ssh $DEPLOY_TARGET "mkdir -p /var/lib/${PACKAGE} && chown glow:glow /var/lib/${PACKAGE}"
}

deliver() {
  scp ${TARGET} $DEPLOY_TARGET:${PACKAGE}.new
  ssh $DEPLOY_TARGET "chmod a+x ${PACKAGE}.new"

  ssh $DEPLOY_TARGET "cat > /tmp/glow-deploy-receiver.sh && chmod a+x /tmp/glow-deploy-receiver.sh" <<EOF
cat > /tmp/${SERVICE_NAME}
cmp /tmp/${SERVICE_NAME} ${SERVICE_PATH} || {
  sudo mv /tmp/${SERVICE_NAME} ${SERVICE_PATH} && sudo systemctl daemon-reload
}
sudo systemctl stop ${SERVICE_NAME}
sudo mv ${PACKAGE}.new /usr/local/bin/${PACKAGE}
sudo systemctl start ${SERVICE_NAME}
EOF

  cat deploy/${PACKAGE}.service | \
    envsubst | \
    ssh $DEPLOY_TARGET /tmp/glow-deploy-receiver.sh
}

deploy() {
  debug "Deploying $PACKAGE"
  check-host
  build-$PACKAGE
  debug "Build ${PACKAGE}: OK"

  md5sum --quiet -c ${TARGET}.md5sum && fail "No change to release binary"

  prepare-$PACKAGE

  deliver

  md5sum ${TARGET} > ${TARGET}.md5sum
}

case "$1" in
  web)
    PACKAGE=glow-web
    TARGET_DIR="./target/release"
    TARGET="${TARGET_DIR}/${PACKAGE}"
    SERVICE_NAME="${PACKAGE}.service"
    SERVICE_PATH="/etc/systemd/system/${SERVICE_NAME}"

    DEPLOY_TARGET=$GLOW_WEB_DEPLOY_TARGET
    ;;
  device)
    PACKAGE="glow-device"
    TRIPPLE="arm-unknown-linux-musleabihf"
    TARGET_DIR="./target/${TRIPPLE}/release"
    TARGET="${TARGET_DIR}/${PACKAGE}"
    SERVICE_NAME="glow.service"
    SERVICE_PATH="/etc/systemd/system/${SERVICE_NAME}"

    DEPLOY_TARGET=$BOBNET_GLOW_DEPLOY_TARGET
    ;;
  both)
    ;;
  *)
    help
    exit 1
    ;;
esac

deploy
