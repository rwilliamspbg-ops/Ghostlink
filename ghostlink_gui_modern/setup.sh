#!/usr/bin/env bash

# Ghostlink GUI Modern - Quick Setup Script

set -e

echo "🚀 Ghostlink Studio Modern GUI Setup"
echo "===================================="
echo ""

# Check Node.js
if ! command -v node &> /dev/null; then
    echo "❌ Node.js not found. Please install Node.js 18+"
    exit 1
fi

echo "✓ Node.js $(node --version)"

# Install dependencies
echo ""
echo "📦 Installing dependencies..."
npm install

# Check if build exists
if [ ! -d "dist" ]; then
    echo ""
    echo "🔨 Building for production..."
    npm run build
fi

echo ""
echo "✅ Setup complete!"
echo ""
echo "📝 To start development server:"
echo "   npm run dev"
echo ""
echo "📝 To preview production build:"
echo "   npm run preview"
echo ""
echo "🐳 To run with Docker:"
echo "   docker build -t ghostlink-gui ."
echo "   docker run -p 3000:3000 ghostlink-gui"
echo ""
echo "🌐 Backend configuration:"
echo "   Edit vite.config.ts to change proxy URL"
echo "   Default: http://127.0.0.1:8003"
