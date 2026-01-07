#!/bin/bash
# Extract Pure JSON from Office365 Logs
# Removes timestamp and tenant name metadata

INPUT_FILE="${1:-output/data.20260105.log}"
OUTPUT_FILE="${2:-output/data.20260105.pure.json}"

if [ ! -f "$INPUT_FILE" ]; then
    echo "❌ ERROR: Input file not found: $INPUT_FILE"
    echo ""
    echo "Usage: $0 [input_file] [output_file]"
    echo ""
    echo "Example:"
    echo "  $0 output/data.20260105.log output/pure.json"
    exit 1
fi

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Office365 Log Converter: Fluentd Format → Pure JSON"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "Input:  $INPUT_FILE"
echo "Output: $OUTPUT_FILE"
echo ""

# Extract third column (pure JSON) from tab-delimited format
cut -f3 "$INPUT_FILE" > "$OUTPUT_FILE"

# Get file sizes
INPUT_SIZE=$(du -h "$INPUT_FILE" | cut -f1)
OUTPUT_SIZE=$(du -h "$OUTPUT_FILE" | cut -f1)
EVENT_COUNT=$(wc -l < "$OUTPUT_FILE")

echo "✅ Conversion complete!"
echo ""
echo "Input size:  $INPUT_SIZE"
echo "Output size: $OUTPUT_SIZE"
echo "Events:      $EVENT_COUNT"
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Output format: One JSON object per line (NDJSON/JSON Lines)"
echo ""
echo "Sample (first event):"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
head -1 "$OUTPUT_FILE" | python3 -m json.tool 2>/dev/null || head -1 "$OUTPUT_FILE"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "✅ Pure JSON logs ready at: $OUTPUT_FILE"
echo ""
