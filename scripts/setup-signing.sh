#!/usr/bin/env bash
# One-time setup: creates a self-signed code-signing certificate ("GRIP Local Signing")
# in your login keychain. Signing with a stable certificate keeps the app's identity the
# same across rebuilds, so macOS keeps the Accessibility permission instead of asking again.
set -euo pipefail

NAME="GRIP Local Signing"
if security find-certificate -c "$NAME" >/dev/null 2>&1; then
  echo "\"$NAME\" already exists in your keychain. Nothing to do."
  exit 0
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cat > "$TMP/cert.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
CNF

/usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
  -keyout "$TMP/key.pem" -out "$TMP/cert.pem" -config "$TMP/cert.cnf" 2>/dev/null
/usr/bin/openssl pkcs12 -export -inkey "$TMP/key.pem" -in "$TMP/cert.pem" \
  -name "$NAME" -out "$TMP/cert.p12" -passout pass:grip 2>/dev/null

# -T lets codesign use the key without a keychain prompt on every build.
security import "$TMP/cert.p12" -k "$HOME/Library/Keychains/login.keychain-db" \
  -P grip -T /usr/bin/codesign
# Trust it for code signing only (macOS asks for your password here).
security add-trusted-cert -p codeSign -k "$HOME/Library/Keychains/login.keychain-db" "$TMP/cert.pem"

echo "Created \"$NAME\". Rebuild with scripts/bundle.sh."
