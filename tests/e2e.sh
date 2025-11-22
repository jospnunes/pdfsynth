#!/bin/bash
set -e

BASE_URL="http://localhost:8080"

echo "Waiting for service to be ready..."
for i in {1..30}; do
    if curl -s "$BASE_URL/health" | grep -q "ok"; then
        echo "Service is up!"
        break
    fi
    sleep 1
done

echo "1. Testing /health..."
curl -f "$BASE_URL/health"
echo ""

echo "2. Testing /render/debug (HTML)..."
curl -X POST "$BASE_URL/render/debug" \
  -H "Content-Type: application/json" \
  -d '{
    "template_html": "<!DOCTYPE html><html><body><h1>Hello {{ name }}</h1></body></html>",
    "data": { "name": "Docker World" }
  }'
echo ""

echo "3. Testing /render (PDF)..."
curl -X POST "$BASE_URL/render" \
  -H "Content-Type: application/json" \
  -d '{
    "template_html": "<!DOCTYPE html><html><body><h1>Hello {{ name }}</h1></body></html>",
    "data": { "name": "PDF World" },
    "options": { "pdf_a": false, "paper_format": "A4" }
  }' --output output_std.pdf
echo "Generated output_std.pdf"

echo "4. Testing /render (PDF/A)..."
curl -X POST "$BASE_URL/render" \
  -H "Content-Type: application/json" \
  -d '{
    "template_html": "<!DOCTYPE html><html><body><h1>Hello {{ name }}</h1></body></html>",
    "data": { "name": "PDF/A World" },
    "options": { "pdf_a": true, "paper_format": "A4" }
  }' --output output_pdfa.pdf
echo "Generated output_pdfa.pdf"

echo "Tests completed."
