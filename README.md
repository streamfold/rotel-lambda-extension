# Rotel Lambda Extension

Rotel Lambda Extension is an advanced AWS Lambda extension layer, built on top of [Rotel](https://github.com/streamfold/rotel)—a lightweight, high-performance and low overhead OpenTelemetry Collector perfectly suited for resource-constrained environments. By minimizing binary size, reducing cold start latency, and lowering memory overhead, this extension optimizes performance and cost efficiency in AWS Lambda deployments.

![Coldstart Comparison](/contrib/coldstarts.png)

_This chart compares cold start times between Rotel, the [OpenTelemetry Lambda](https://github.com/open-telemetry/opentelemetry-lambda/blob/main/collector/README.md), and the [Datadog OTEL Lambda](https://docs.datadoghq.com/serverless/aws_lambda/opentelemetry/?tab=python) layers. Check out the benchmark code [here](https://github.com/streamfold/python-lambda-benchmark)._ 

The Rotel Lambda Extension integrates with the Lambda [TelemetryAPI](https://docs.aws.amazon.com/lambda/latest/dg/telemetry-api.html) to collect **function logs** and **extension logs** and will export them to the configured exporter. This can reduce your Lambda observability costs if you combine it with [disabling CloudWatch Logs](#disabling-cloudwatch-logs). 

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
- eu-central-1
- ap-southeast-2

The layer supports the Amazon Linux 2023
[Lambda runtime](https://docs.aws.amazon.com/lambda/latest/dg/lambda-runtimes.html#runtimes-supported)
(`provided.al2023`).

The _{version}_ field should match the integer value for the latest release on the
[Releases](https://github.com/streamfold/rotel-lambda-extension/releases) page,
for example `v12-alpha` should use `12` as the version.

## Auto instrumentation

The Rotel Lambda layer can be used alongside the language support extension layers, found [here](https://github.com/open-telemetry/opentelemetry-lambda?tab=readme-ov-file#extension-layer-language-support). The default Rotel OTLP receiver configuration matches the defaults used for OTEL auto-instrumentation.

To use a language layer, pick the extension layer ARN that matches your runtime language and include it **in addition** to the Rotel layer ARN above. You will need to set `AWS_LAMBDA_EXEC_WRAPPER` so that your code is auto-instrumented on start up. Make sure to consult the documentation for your instrumentation layer.

To see how this works in practice, check out this Node.js
[example ✨](https://github.com/streamfold/nodejs-aws-lambda-example). 

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
ROTEL_OTLP_EXPORTER_CUSTOM_HEADERS="Authorization=Bearer ${AXIOM_API_KEY},X-Axiom-Dataset=${AXIOM_DATASET}"
```

The values `${AXIOM_API_KEY}` and `${AXIOM_DATASET}` will be resolved from the environment of the function,
allowing you to set the secret values in your AWS Lambda function definition and out of the on-disk file.

### Secrets

Secret values can be retrieved from **[AWS Secrets Manager](https://aws.amazon.com/secrets-manager/)** or from **[AWS Parameter Store](https://docs.aws.amazon.com/systems-manager/latest/userguide/systems-manager-parameter-store.html)** by specifying the full
ARN of the stored secret as the environment variable name. This allows you to keep secret values out of configuration
files.

**AWS Secrets Manager Example**

```shell
ROTEL_OTLP_EXPORTER_ENDPOINT=https://api.axiom.co
ROTEL_OTLP_EXPORTER_PROTOCOL=http
ROTEL_OTLP_EXPORTER_CUSTOM_HEADERS="Authorization=Bearer ${arn:aws:secretsmanager:us-east-1:123377354456:secret:axiom-api-key-r1l7G9},X-Axiom-Dataset=${AXIOM_DATASET}"
```

Secrets retrieved from AWS Secrets Manager also support JSON encoded secret key/value pairs. The secret
value can be retrieved by suffixing the ARN with a `#key-name` where `key-name` is the top-level JSON key. For example,
if the secret named `axiom-r1l7G9` contained:

```json
{
  "key": "1234abcd",
  "dataset": "my-dataset"
}
```

Then the following example would extract those values:
```shell
ROTEL_OTLP_EXPORTER_ENDPOINT=https://api.axiom.co
ROTEL_OTLP_EXPORTER_PROTOCOL=http
ROTEL_OTLP_EXPORTER_CUSTOM_HEADERS="Authorization=Bearer ${arn:aws:secretsmanager:us-east-1:123377354456:secret:axiom-r1l7G9#key},X-Axiom-Dataset=${arn:aws:secretsmanager:us-east-1:123377354456:secret:axiom-r1l7G9#dataset}"
```


**AWS Parameter Store Example**

```shell
ROTEL_OTLP_EXPORTER_ENDPOINT=https://api.axiom.co
ROTEL_OTLP_EXPORTER_PROTOCOL=http
ROTEL_OTLP_EXPORTER_CUSTOM_HEADERS="Authorization=Bearer ${arn:aws:ssm:us-east-1:123377354456:parameter/axiom-api-key},X-Axiom-Dataset=${AXIOM_DATASET}"
```

**Permissions:**

You must ensure the following IAM permissions exist for your Lambda runtime execution role:

* Secrets Manager
  - [`secretsmanager:GetSecretValue`](https://docs.aws.amazon.com/secretsmanager/latest/apireference/API_GetSecretValue.html)
  - [`secretsmanager:BatchGetSecretValue`](https://docs.aws.amazon.com/secretsmanager/latest/apireference/API_BatchGetSecretValue.html)
* Parameter Store
  - [`ssm:GetParameters`](https://docs.aws.amazon.com/systems-manager/latest/APIReference/API_GetParameters.html)

Secrets must be stored as a plaintext secret string value for AWS Secrets Manager and as a SecureString for AWS Parameter Store. 

**NOTE**:

AWS API calls can increase cold start latency by 100-150 ms even when made within the same region, so be
mindful of that impact when retrieving secrets. Secrets are retrieved in batches up to 10, so retrieving
multiple secret values should not take longer than a single secret.

Secrets are only retrieved on initialization, so subsequent invocations are not impacted.

### Default resource attributes

Log messages forwarded with the TelemetryAPI will automatically use a `service.name` equal to the AWS Lambda function name. Trace spans will default to the configured SDK value. You can set `service.name`, and any other resource attribute, with the following environment variable:

```shell
ROTEL_OTEL_RESOURCE_ATTRIBUTES="service.name=my-lambda-api,service.version=2.0.0"
```

This will insert or replace those resource attributes on all traces, logs, and metrics. See Rotel
[docs](https://github.com/streamfold/rotel?tab=readme-ov-file#setting-resource-attributes) for more info.

## Disabling CloudWatch Logs

By default, AWS Lambda will send all Lambda logs to Amazon CloudWatch. To reduce costs, you may want to disable those logs if you are forwarding your logs to an external logging provider.

1. Open the AWS Console and navigate to AWS Lambda
2. Navigate to your Lambda function
3. Select Configuration -> Permissions
4. Click the execution role under "Role Name" to pop over to the IAM console
5. Edit the role in the IAM console and remove any `logs:*` actions
   - if you are using a custom policy, edit the policy to remove `logs:*` actions
   - if you are using an AWS Managed policy, like `AWSLambdaBasicExecutionRole`, remove it from the role
6. Save the role and your next execution should not send logs to CloudWatch

## Adaptive Flushing

This extension uses an **adaptive flushing model**, similar to the one implemented in [Datadog's new Rust Lambda extension](https://www.datadoghq.com/blog/engineering/datadog-lambda-extension-rust/).

On the initial invocation after a cold start, the extension flushes all telemetry data **at the end** of each function invocation. This ensures minimal delay in telemetry availability. However, because the flush happens *after* the invocation completes, it can slightly increase the billed duration of the function.

If the extension detects a regular invocation pattern—such as invocations occurring at least once per minute—it will switch to **periodic flushing at the start** of each invocation. This overlaps the flush operation with the function’s execution time, reducing the likelihood of added billed duration due to telemetry flushing.

For long-running invocations, a **global backup timer** is used to flush telemetry periodically. This timer is reset whenever a regular flush occurs, ensuring that telemetry is still sent even if invocation patterns become irregular.

## Community

Want to chat about this project, share feedback, or suggest improvements? Join our [Discord server](https://discord.gg/reUqNWTSGC)! Whether you're a user of this project or not, we'd love to hear your thoughts and ideas. See you there! 🚀

## Developing

See [DEVELOPING](/DEVELOPING.md) for developer instructions.

---

Built with ❤️ by Streamfold.
