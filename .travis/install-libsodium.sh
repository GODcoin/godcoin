#!/usr/bin/env bash
set -e

VERSION=$LIBSODIUM_VERSION
if [ ! -d "$HOME/libsodium-$VERSION/lib" ]; then
  wget "https://github.com/jedisct1/libsodium/releases/download/$VERSION/libsodium-$VERSION.tar.gz"
  tar xvfz "libsodium-$VERSION.tar.gz"
  cd "libsodium-$VERSION"
  ./configure --prefix="$HOME/libsodium-$VERSION"
  make
  make install
else
  echo 'Using cached directory.'
fi
