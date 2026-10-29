#!/usr/bin/env bash
# Runs as root in an ephemeral distro container; packages are mounted read-only.
set -euo pipefail
shopt -s nullglob
case "$PACKAGE_FORMAT" in
  deb)
    packages=(/packages/*.deb)
    test "${#packages[@]}" -eq 1
    export DEBIAN_FRONTEND=noninteractive
    apt-get update
    apt-get install --no-install-recommends -y \
      -o 'Dpkg::Options::=--path-include=/usr/share/doc/tabius-desktop/*' "${packages[0]}"
    ;;
  rpm)
    packages=(/packages/*.rpm)
    test "${#packages[@]}" -eq 1
    dnf install --setopt=install_weak_deps=False -y "${packages[0]}"
    test "$(rpm -q --queryformat '%{LICENSE}' tabius-desktop)" = GPL-3.0-only
    ;;
  archlinux)
    packages=(/packages/*.pkg.tar.zst)
    test "${#packages[@]}" -eq 1
    pacman -Syu --noconfirm
    printf '\n[options]\nNoExtract = !usr/share/doc/tabius-desktop !usr/share/doc/tabius-desktop/*\n' >> /etc/pacman.conf
    pacman -U --noconfirm "${packages[0]}"
    ;;
  *) echo "Unknown package format: $PACKAGE_FORMAT" >&2; exit 1 ;;
esac

test "$(tabius-desktop --version)" = "tabius-desktop $PACKAGE_VERSION"
tabius-desktop --help
test -f /usr/share/applications/dev.tabius.desktop.desktop
test -f /usr/share/icons/hicolor/scalable/apps/tabius-desktop.svg
test -f /usr/share/doc/tabius-desktop/LICENSE
mkdir -p /root/.local/share/dev.tabius.desktop
echo retained > /root/.local/share/dev.tabius.desktop/package-test-marker
case "$PACKAGE_FORMAT" in
  deb) apt-get remove -y tabius-desktop ;;
  rpm) dnf remove -y tabius-desktop ;;
  archlinux) pacman -R --noconfirm tabius-desktop ;;
esac
test ! -e /usr/bin/tabius-desktop
test "$(cat /root/.local/share/dev.tabius.desktop/package-test-marker)" = retained
