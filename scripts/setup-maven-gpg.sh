#!/usr/bin/env bash
set -euo pipefail

echo "=== Lavi Maven Central PGP Key Setup ==="
read -r -p "Enter your name for the PGP key (e.g. Mike Burton): " GPG_NAME
read -r -p "Enter your email for the PGP key: " GPG_EMAIL
read -r -s -p "Enter a passphrase for the PGP key: " GPG_PASSPHRASE
echo ""

gpg --batch --gen-key <<EOF
Key-Type: RSA
Key-Length: 4096
Subkey-Type: RSA
Subkey-Length: 4096
Name-Real: ${GPG_NAME}
Name-Email: ${GPG_EMAIL}
Expire-Date: 2y
Passphrase: ${GPG_PASSPHRASE}
%commit
EOF

KEY_ID=$(gpg --list-secret-keys --with-colons "${GPG_EMAIL}" | awk -F: '/^sec:/ { print $5; exit }')
echo "Generated PGP Key ID: ${KEY_ID}"

echo "Publishing public key to keys.openpgp.org and keyserver.ubuntu.com..."
gpg --keyserver hkps://keys.openpgp.org --send-keys "${KEY_ID}"
gpg --keyserver hkps://keyserver.ubuntu.com --send-keys "${KEY_ID}"

echo ""
echo "=== Copy the armored private key below into GitHub Secret: MAVEN_GPG_PRIVATE_KEY ==="
gpg --armor --pinentry-mode loopback --passphrase "${GPG_PASSPHRASE}" --export-secret-keys "${KEY_ID}"
echo ""
echo "=== Also set GitHub Secret MAVEN_GPG_PASSPHRASE to the passphrase you entered ==="
