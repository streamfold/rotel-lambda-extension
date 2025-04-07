# Rotel Lambda Extension

Rotel Lambda Extension is an AWS Lambda extension that includes the Rotel Lightweight OpenTelemetry Collector.

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install)
- [Cargo Lambda](https://www.cargo-lambda.info/guide/installation.html)

## Building

To build the project for production, run `cargo lambda build --extension --release`. Remove the `--release` flag to build for development.

Read more about building your lambda extension in [the Cargo Lambda documentation](https://www.cargo-lambda.info/commands/build.html#extensions).

## Deploying

To deploy the project, run `cargo lambda deploy --extension`. This will upload the extension to your AWS account as an AWS Lambda Layer.

Read more about deploying your lambda extension in [the Cargo Lambda documentation](https://www.cargo-lambda.info/commands/deploy.html#extensions).
