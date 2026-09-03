#!/usr/bin/env bash
#
# Sign a local macOS build with a stable identity, so the Keychain stops
# asking at every launch.
#
# The linker ad-hoc signs a debug binary, and its cdhash changes with every
# build. When ai-buddy first saves the Director API key, macOS writes that hash
# into the item's access control list twice — once for the trusted application
# and once for the partition list — so the next build matches neither, and a
# launch that reads the key costs two dialogs. Always Allow only pins the hash
# that is about to change. Signed with a certificate instead, the ACL names the
# identity (`identifier ai-buddy and certificate root = H"..."`), which a
# rebuild does not disturb.
#
# The certificate is self-signed, created here on first run, and trusted by
# nothing else: Gatekeeper does not accept it, and it is not the Developer ID a
# release needs (#283). It is imported with `-A` so signing needs no password
# every build, which also means any local process can sign as this identity —
# acceptable for a certificate whose only authority is over this Mac's own
# keychain ACLs, and the reason this script is for development alone.
#
# Usage: scripts/dev-sign.sh [path]
#   Signs target/debug/ai-buddy unless given another binary or .app bundle.
#   Cargo replaces the signature on every build, so this runs after each one:
#
#     cargo build -p ai-buddy && scripts/dev-sign.sh && ./target/debug/ai-buddy
#
# A key saved before the first signed run keeps its old ACL. Clear it in
# Settings and save it once more from a signed build to stop the prompts.

set -euo pipefail
cd "$(dirname "$0")/.."

if [[ "$(uname -s)" != Darwin ]]; then
  echo "dev-sign: macOS only; nothing to sign on $(uname -s)" >&2
  exit 0
fi

IDENTITY="ai-buddy dev signing"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"
TARGET="${1:-target/debug/ai-buddy}"

if [[ ! -e $TARGET ]]; then
  echo "dev-sign: no $TARGET — build it first" >&2
  exit 1
fi

if ! security find-certificate -c "$IDENTITY" "$KEYCHAIN" > /dev/null 2>&1; then
  echo "dev-sign: creating the '$IDENTITY' certificate in the login keychain"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
    -keyout "$tmp/key.pem" -out "$tmp/cert.pem" \
    -subj "/CN=$IDENTITY/O=ai-buddy" \
    -addext "basicConstraints=critical,CA:false" \
    -addext "keyUsage=critical,digitalSignature" \
    -addext "extendedKeyUsage=critical,codeSigning" 2> /dev/null
  # 3DES rather than the OpenSSL 3 default: the macOS importer cannot read a
  # PKCS#12 encrypted with AES.
  openssl pkcs12 -export -name "$IDENTITY" \
    -inkey "$tmp/key.pem" -in "$tmp/cert.pem" -out "$tmp/id.p12" \
    -passout pass:ai-buddy \
    -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1
  security import "$tmp/id.p12" -k "$KEYCHAIN" -P ai-buddy -T /usr/bin/codesign -A
fi

# A bundle carries the frameworks Tauri links; the raw binary carries nothing.
deep=()
[[ $TARGET == *.app ]] && deep=(--deep)

codesign --force "${deep[@]}" --sign "$IDENTITY" "$TARGET"
codesign -dvvv "$TARGET" 2>&1 | grep -E '^(Authority|CDHash)='
