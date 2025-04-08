# Rotel Lambda Extension

Rotel Lambda Extension is an AWS Lambda extension layer that includes the [Rotel](https://github.com/streamfold/rotel)
Lightweight OpenTelemetry Collector.

## Using

For **alpha** releases, chose the Lambda layer that matches your Lambda runtime architecture:

- x86-64/amd64: `arn:aws:lambda:{region}:418653438961:layer:rotel-extension-amd64-alpha:{version}`
- arm64: `arn:aws:lambda:{region}:418653438961:layer:rotel-extension-arm64-alpha:{version}`

Currently supported regions (if you don't see yours, let us know!):
- us-east-1
- us-east-2
- us-west-2

The _{version}_ field should match the integer value for the latest release on the
[Releases](https://github.com/streamfold/rotel-lambda-extension/releases) page,
for example `v12-alpha` should use `12` as the version.

## Configuration



## Developing

See [DEVELOPING](/DEVELOPING.md) for developer instructions.