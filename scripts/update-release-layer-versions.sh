#!/bin/bash
set -e

# Script to collect Lambda layer versions across all regions and update GitHub release notes
# Usage: ./update-release-layer-versions.sh <tag-name>
#
# Example: ./update-release-layer-versions.sh v0.1.0
#
# This script:
# 1. Queries AWS Lambda for the latest layer versions across all regions
# 2. Generates a markdown table with the version information
# 3. Appends the table to the GitHub release notes

if [ $# -lt 1 ]; then
    echo "Usage: $0 <tag-name>"
    echo "Example: $0 v0.1.0"
    exit 1
fi

TAG_NAME="$1"

# Check required environment variables
if [ -z "$GITHUB_TOKEN" ] && [ -z "$GH_TOKEN" ]; then
    echo "Error: GITHUB_TOKEN or GH_TOKEN environment variable must be set"
    exit 1
fi

# Define regions and architectures
REGIONS=(
    us-east-1
    us-east-2
    us-west-1
    us-west-2
    ca-central-1
    eu-central-1
    eu-north-1
    eu-west-1
    eu-west-2
    eu-west-3
    ap-southeast-1
    ap-southeast-2
    ap-northeast-1
    ap-northeast-2
    ap-south-1
    sa-east-1
)

ARCHS=("x86-64" "arm64")

echo "============================================"
echo "Collecting Lambda Layer Versions"
echo "============================================"
echo ""

# Create temporary file for results
TMP_CSV=$(mktemp)
TMP_MD=$(mktemp)

echo "region,x86-64,arm64" > "$TMP_CSV"

# Iterate through regions
for region in "${REGIONS[@]}"; do
    echo "Checking region: $region"
    row="$region"

    # Check each architecture
    for arch in "${ARCHS[@]}"; do
        if [ "$arch" == "x86-64" ]; then
            layer_name="rotel-extension-amd64"
        else
            layer_name="rotel-extension-arm64"
        fi

        echo "  Checking layer: $layer_name"

        # Get the latest layer version
        version=$(aws lambda list-layer-versions \
            --layer-name "$layer_name" \
            --region "$region" \
            --query 'LayerVersions[0].Version' \
            --output text 2>/dev/null || echo "N/A")

        if [ "$version" == "None" ] || [ -z "$version" ]; then
            version="N/A"
        fi

        row="$row,$version"
        echo "    Version: $version"
    done

    echo "$row" >> "$TMP_CSV"
    echo ""
done

echo "============================================"
echo "Generating Markdown Table"
echo "============================================"
echo ""

# Generate markdown table
cat > "$TMP_MD" << 'MDEOF'

---

## Lambda Layer Versions by Region

This layer can be installed with the following Lambda layer ARNs, choose the right architecture and the
right version for your region based on the table below.

### `x86-64`/`amd64`
```
arn:aws:lambda:{region}:418653438961:layer:rotel-extension-amd64:{version}
```

### `arm64`
```
arn:aws:lambda:{region}:418653438961:layer:rotel-extension-arm64:{version}
```

### Region versions

| Region | x86-64 | arm64 |
|--------|--------|-------|
MDEOF

# Skip header line and process CSV
tail -n +2 "$TMP_CSV" | while IFS=',' read -r region x86 arm; do
    echo "| $region | $x86 | $arm |" >> "$TMP_MD"
done

echo "" >> "$TMP_MD"

# Display the generated table
echo "Generated markdown table:"
cat "$TMP_MD"
echo ""

echo "============================================"
echo "Updating GitHub Release"
echo "============================================"
echo ""

# Check if gh CLI is available
if ! command -v gh &> /dev/null; then
    echo "Error: GitHub CLI (gh) is not installed"
    echo "Install it from: https://cli.github.com/"
    exit 1
fi

# Get current release body
echo "Fetching current release notes for $TAG_NAME..."
CURRENT_BODY=$(gh release view "$TAG_NAME" --json body --jq '.body')

if [ $? -ne 0 ]; then
    echo "Error: Could not fetch release $TAG_NAME"
    echo "Make sure the release exists and you have proper permissions"
    rm -f "$TMP_CSV" "$TMP_MD"
    exit 1
fi

# Read the new content
NEW_CONTENT=$(cat "$TMP_MD")

# Combine bodies
UPDATED_BODY="${CURRENT_BODY}${NEW_CONTENT}"

# Update the release
echo "Updating release notes..."
gh release edit "$TAG_NAME" --notes "$UPDATED_BODY"

if [ $? -eq 0 ]; then
    echo ""
    echo "✅ Successfully updated release notes with layer version information"
else
    echo ""
    echo "❌ Failed to update release notes"
    rm -f "$TMP_CSV" "$TMP_MD"
    exit 1
fi

# Cleanup
rm -f "$TMP_CSV" "$TMP_MD"

echo ""
echo "Done!"
