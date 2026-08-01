#!/bin/bash

# Define proxy settings
export http_proxy=http://127.0.0.1:8080
export https_proxy=http://127.0.0.1:8080
export NO_PROXY=localhost,127.0.0.1
export BANGO_PREMIUM=1

echo "Proxy set to 127.0.0.1:8080"
echo "Starting Tauri dev server..."

# Run the dev command
# Using 'exec' ensures signals (like Ctrl+C) are passed correctly
exec npm run tauri dev