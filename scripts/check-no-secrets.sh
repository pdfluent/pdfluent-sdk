#!/bin/bash
# Pre-commit hook: check for private keys in staged files
if git diff --cached --diff-filter=ACM | grep -qE "BEGIN.*PRIVATE KEY|PRIVATE_KEY.*=.*[A-Za-z0-9+/]{20}"; then
  echo "ERROR: Private key detected in staged files!"
  echo "Remove the key and use environment variables instead."
  exit 1
fi
