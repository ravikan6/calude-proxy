#!/bin/bash

# Set OpenAI API key (or use a mock)
export OPENAI_API_KEY="test-key-for-now"

# Build the project
cargo build --release

# Start the server in the background
./target/release/llm-proxy &
SERVER_PID=$!

# Wait for server to start
sleep 2

# Test the endpoint
echo "Testing the proxy..."
curl -X POST http://localhost:8082/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-3.5-turbo",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'

# Stop the server
kill $SERVER_PID
wait $SERVER_PID 2>/dev/null

echo "Test complete!"
