# Rotel Lambda Extension

Rotel Lambda Extension is an advanced AWS Lambda extension layer, built on top of [Rotel](https://github.com/streamfold/rotel)—a lightweight OpenTelemetry Collector designed specifically for resource-constrained environments. By minimizing binary size, reducing cold start latency, and lowering memory overhead, this extension optimizes performance and cost efficiency in AWS Lambda deployments.

![Coldstart Comparison](/contrib/coldstarts.png)
_This chart compares cold start times between Rotel and the [OpenTelemetry Lambda](https://github.com/open-telemetry/opentelemetry-lambda/blob/main/collector/README.md) layer. Check out the benchmark code [here](https://github.com/streamfold/python-lambda-benchmark)._ 

## Using

Choose the Lambda layer that matches your Lambda runtime architecture (**alpha** versions shown):

**x86-64/amd64**
```
arn:aws:lambda:{region}:418653438961:layer:rotel-extension-amd64-alpha:{version}
```

**arm64** 
```
arn:aws:lambda:{region}:418653438961:layer:rotel-extension-arm64-alpha:{version}
```

The layer is deployed in the following AWS regions (if you don't see yours, let us know!):
- us-east-1
- us-east-2
- us-west-2

The layer supports the Amazon Linux 2023
[Lambda runtime](https://docs.aws.amazon.com/lambda/latest/dg/lambda-runtimes.html#runtimes-supported)
(`provided.al2023`).

The _{version}_ field should match the integer value for the latest release on the
[Releases](https://github.com/streamfold/rotel-lambda-extension/releases) page,
for example `v12-alpha` should use `12` as the version.

## Auto instrumentation

The Rotel Lambda layer can be used alongside the language support extension layers, found [here](https://github.com/open-telemetry/opentelemetry-lambda?tab=readme-ov-file#extension-layer-language-support). The default Rotel OTLP receiver configuration matches the defaults used for OTEL auto-instrumentation.

To use a language layer, pick the extension layer ARN that matches your runtime language and include it in additional to the Rotel layer ARN above. Consult the documentation for each language layer to identify how to set `AWS_LAMBDA_EXEC_WRAPPER` so that your code is auto-instrumented on start up.

## Configuration

The Rotel Lambda Extension is configured using the same environment variables documented
for the Rotel collector,
[documented here](https://github.com/streamfold/rotel?tab=readme-ov-file#configuration).

To ease configuration for Lambda environments, you can set `ROTEL_ENV_FILE` to the path
name of a file and that file will be interpreted as an `.env` file. For example, set
`ROTEL_ENV_FILE=/var/task/rotel.env` and include the following `rotel.env` file in your
function bundle:
```shell
ROTEL_OTLP_EXPORTER_ENDPOINT=https://api.axiom.co
ROTEL_OTLP_EXPORTER_PROTOCOL=http
ROTEL_OTLP_EXPORTER_CUSTOM_HEADERS=Authorization=Bearer ${AXIOM_API_KEY},X-Axiom-Dataset=${AXIOM_DATASET}
```

The values `${AXIOM_API_KEY}` and `${AXIOM_DATASET}` will be resolved from the environment of the function,
allowing you to set the secret values in your AWS Lambda function definition and out of the on-disk file. 

## Developing

See [DEVELOPING](/DEVELOPING.md) for developer instructions.