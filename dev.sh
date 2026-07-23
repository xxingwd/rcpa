#!/bin/bash

# Exit immediately if a command exits with a non-zero status
set -e

# Setup clean termination of background jobs on exit
trap "trap - SIGTERM && kill -- -$$" SIGINT SIGTERM EXIT

echo "⚙️  Building React frontend..."
cd frontend
npm install
npm run build
cd ..

echo "🚀 Starting RCPA Gateway Backend (cargo run)..."
cargo run -- --token local-admin-token --data-dir data --port 15000 --log-level info &
BACKEND_PID=$!

echo "🚀 Starting Vite Dev Server..."
cd frontend
npm run dev -- --host 0.0.0.0 &
FRONTEND_PID=$!

echo "💡 App is running!"
echo "   - Frontend (Vite with HMR): http://localhost:5173"
echo "   - Backend (Gateway Port):   http://localhost:15000"
echo "Press Ctrl+C to stop both."

wait
