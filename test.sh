#!/bin/bash

set -e

cleanup() {
  echo "Cleaning up..."
  docker stop near-node 2>/dev/null || true
  rm -r sandbox
  echo "Cleanup completed."
}

# Cleanup on exit
trap cleanup ERR EXIT


echo "Starting NEAR Protocol node in Docker container..."
docker run -it --rm --platform linux/amd64 -v ./sandbox:/srv nearprotocol/nearcore:2.6.0 neard --home /srv init

# Modify config.json to enable archive features
CONFIG_FILE="$(pwd)/sandbox/config.json"
echo "Modifying config.json file to enable archive features..."
TMP_FILE=$(mktemp)
jq '.archive = true | .cold_store = {} | .save_trie_changes = true' "$CONFIG_FILE" > "$TMP_FILE" && mv "$TMP_FILE" "$CONFIG_FILE"


echo "Starting the NEAR node..."
docker run -d --name near-node --rm --platform linux/amd64 -v ./sandbox:/srv -p 3030:3030 nearprotocol/nearcore:2.6.0 neard --home /srv run


echo "Waiting for NEAR node to start..."
for i in {1..30}; do
  if curl -s http://localhost:3031/status > /dev/null; then
    echo "NEAR node is ready!"
    break
  fi
  [ $i -eq 30 ] && { echo "Error: Node failed to start"; exit 1; }
  echo "Waiting... ($i/30)"
  sleep 5
done


echo "Initializing state using npm commands..."
cd init_script
npm install
npm run init
npm run deploy
npm run load
npm run add_proposal
npm run vote 0 1
cd ../


echo "Running rocket tests..."
cargo test

echo "Tests completed!"
