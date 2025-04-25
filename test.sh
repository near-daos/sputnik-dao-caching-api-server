#!/bin/bash

set -e

cleanup() {
  echo "Cleaning up..."
  docker stop near-node 2>/dev/null || true
  rm -r sandbox
  rm cache.bin
  echo "Cleanup completed."
}

# Cleanup on exit
trap cleanup ERR EXIT

echo "Copying sandbox_init to sandbox..."
cp -r sandbox_init sandbox

echo "Starting the NEAR node..."
docker run -d --name near-node --rm --platform linux/amd64 -v ./sandbox:/srv -p 3030:3030 nearprotocol/nearcore:2.6.0 neard --home /srv run

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

echo "Running rocket tests..."
cargo test

echo "Tests completed!"
