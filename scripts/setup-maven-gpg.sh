#!/usr/bin/env bash
set -euo pipefail

echo "=== Lavi Maven Central PGP Key Setup ==="
read -r -p "Enter your name for the PGP key [Mike Burton]: " GPG_NAME
GPG_NAME="${GPG_NAME:-Mike Burton}"

read -r -p "Enter your email for the PGP key [mb-sonatype@niskala.org]: " GPG_EMAIL
GPG_EMAIL="${GPG_EMAIL:-mb-sonatype@niskala.org}"

read -r -s -p "Enter a passphrase for the PGP key (or press Enter for none): " GPG_PASSPHRASE
echo ""

if [[ -n "${GPG_PASSPHRASE}" ]]; then
  PASSPHRASE_DIRECTIVE="Passphrase: ${GPG_PASSPHRASE}"
else
  PASSPHRASE_DIRECTIVE="%no-protection"
fi

gpg --batch --gen-key <<EOF
Key-Type: RSA
Key-Length: 4096
Subkey-Type: RSA
Subkey-Length: 4096
Name-Real: ${GPG_NAME}
Name-Email: ${GPG_EMAIL}
Expire-Date: 2y
${PASSPHRASE_DIRECTIVE}
%commit
EOF

KEY_ID=$(gpg --list-secret-keys --with-colons "${GPG_EMAIL}" | awk -F: '/^sec:/ { print $5; exit }')
echo "Generated PGP Key ID: ${KEY_ID}"

echo "Publishing public key to keys.openpgp.org and keyserver.ubuntu.com..."
gpg --keyserver hkps://keys.openpgp.org --send-keys "${KEY_ID}"
gpg --keyserver hkps://keyserver.ubuntu.com --send-keys "${KEY_ID}"

if [[ -n "${GPG_PASSPHRASE}" ]]; then
  ARMORED_KEY=$(gpg --armor --pinentry-mode loopback --passphrase "${GPG_PASSPHRASE}" --export-secret-keys "${KEY_ID}")
else
  ARMORED_KEY=$(gpg --armor --export-secret-keys "${KEY_ID}")
fi

if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
  echo "Setting GitHub Actions secrets MAVEN_GPG_PRIVATE_KEY and MAVEN_GPG_PASSPHRASE via gh CLI..."
  printf "%s\n" "${ARMORED_KEY}" | gh secret set MAVEN_GPG_PRIVATE_KEY -R emmby/lavi
  printf "%s" "${GPG_PASSPHRASE:-}" | gh secret set MAVEN_GPG_PASSPHRASE -R emmby/lavi
  echo "✅ Uploaded MAVEN_GPG_PRIVATE_KEY and MAVEN_GPG_PASSPHRASE to GitHub Secrets (emmby/lavi)."
else
  echo ""
  echo "=== Copy the armored private key below into GitHub Secret: MAVEN_GPG_PRIVATE_KEY ==="
  printf "%s\n" "${ARMORED_KEY}"
  echo ""
  echo "=== Also set GitHub Secret MAVEN_GPG_PASSPHRASE to the passphrase you entered ==="
fi
