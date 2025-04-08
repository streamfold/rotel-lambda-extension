#!/bin/bash

if [ $# -lt 1 ]; then
    echo "Usage: $0 ARN [...ARN]"
    exit 1
fi

for ARN in "$@"; do
  # Extract region (us-east-1)
  LAYER_REGION=$(echo $ARN | cut -d':' -f4)

  # Extract layer name (rotel-extension)
  LAYER_NAME=$(echo $ARN | cut -d':' -f7)

  # Extract version number (2)
  LAYER_VERSION=$(echo $ARN | cut -d':' -f8)

  echo "Adding layer version permission for layer name: $LAYER_NAME, region: $LAYER_REGION, version: $LAYER_VERSION"

  aws lambda add-layer-version-permission \
    --layer-name "$LAYER_NAME" \
    --version-number "$LAYER_VERSION" \
    --statement-id add-public-access \
    --region "$LAYER_REGION" \
    --principal "*" \
    --action lambda:GetLayerVersion
done