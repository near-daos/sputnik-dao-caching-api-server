#!/bin/bash

set -e

cleanup() {
  echo "Cleaning up..."
  docker stop near-node 2>/dev/null || true
  echo "Cleanup completed."
}

# Cleanup on exit
trap cleanup ERR

rm -r sandbox_init || true

echo "Starting NEAR Protocol node in Docker container..."
docker run -it --rm --platform linux/amd64 -v ./sandbox_init:/srv nearprotocol/nearcore:2.6.0 neard --home /srv init

# Modify config.json to enable archive features
CONFIG_FILE="$(pwd)/sandbox_init/config.json"
echo "Modifying config.json file to enable archive features..."
TMP_FILE=$(mktemp)
jq '.archive = true | .cold_store = {} | .save_trie_changes = true' "$CONFIG_FILE" > "$TMP_FILE" && mv "$TMP_FILE" "$CONFIG_FILE"

echo "Starting the NEAR node..."
docker run -d --name near-node --rm --platform linux/amd64 -v ./sandbox_init:/srv -p 3030:3030 nearprotocol/nearcore:2.6.0 neard --home /srv run

echo "Waiting for NEAR node to start..."
for i in {1..30}; do
  if curl -s http://localhost:3030/status > /dev/null; then
    echo "NEAR node is ready!"
    break
  fi
  [ $i -eq 30 ] && { echo "Error: Node failed to start"; exit 1; }
  echo "Waiting... ($i/30)"
  sleep 5
done

echo "Initializing state using npm commands..."
export VALIDATOR_KEY_PATH="../sandbox_init/validator_key.json"
cd init_script
npm install
npm run init
npm run deploy
npm run load
npm run add_proposal
npm run add_proposal
npm run vote 0 25
cd ../

echo "Stopping NEAR node..."
docker stop near-node

echo "Initialization completed successfully!" 