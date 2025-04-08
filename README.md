# Rotel Lambda Extension

Rotel Lambda Extension is an AWS Lambda extension that includes the Rotel Lightweight OpenTelemetry Collector.

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install)
- [Cargo Lambda](https://www.cargo-lambda.info/guide/installation.html)

## Building

To build the project for production, run `cargo lambda build --extension --release`. Remove the `--release` flag to build for development.

Read more about building your lambda extension in [the Cargo Lambda documentation](https://www.cargo-lambda.info/commands/build.html#extensions).

## Deploy and Publish

When a release is created, the `release.yml` Github action will deploy and publish a new Lambda layer for both x86-64 and arm64 architectures.
The layer will be published to multiple regions, controlled by the regions matrix in the action script.

Release names/tags should follow a specific pattern:

For *alpha releases*:
- `v1-alpha`
- `v2-alpha`
- ...
This will result in layers: `rotel-extension-alpha:1`, `rotel-extension-alpha:2`, etc. The last value is the Lambda layer version.

For production releases:
- `v1`
- `v2`
- ...
This will result in layers: `rotel-extension:1`, `rotel-extension:2`, etc.

**NOTE**: There is no way to control the version number that AWS generates for a new Lambda layer. Therefore, we can only
rely on the auto-incrementing values to match the release name if we follow the same incrementing version scheme.

For the *arm64* architecture, the extension is named `rotel-extension-arm64-alpha` and `rotel-extension-arm64`.
