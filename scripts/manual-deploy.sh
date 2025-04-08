#!/bin/bash

if [ $# -ne 4 ]; then
  echo "Usage: $0 arch layer-name region how-many"
  exit 1
fi

set -e

ARCH="$1"
shift

if [ "$ARCH" != "x86-64" -a "$ARCH" != "arm64" ]; then
  echo "Invalid arch: $ARCH"
  exit 1
fi

LAYER_NAME="$1"
shift

REGION="$1"
shift

HOW_MANY="$1"
shift

echo "Deploying arch $ARCH as $LAYER_NAME to $REGION $HOW_MANY times...sleeping 5 seconds"
sleep 5

export AWS_PROFILE=AdministratorAccess-418653438961

OUT=/tmp/lambda-deploy.out

rm -f $OUT
for ((i = 0; i < $HOW_MANY; ++i)); do
  echo "Deploying $LAYER_NAME for iter $i"
  echo
  AWS_REGION="$REGION" cargo lambda deploy --extension --region "$REGION" --lambda-dir "target/lambda/${ARCH}" \
    --binary-name rotel-extension "$LAYER_NAME" | tee -a $OUT
done

./scripts/publish-lambda-version.sh $( grep 'extension arn' "$OUT"  | awk '{print $4}' )


