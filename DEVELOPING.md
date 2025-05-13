# Developing

Prerequisites:
- [Cargo Lambda](https://www.cargo-lambda.info/): Build and deploy tool

## Building

```shell
make build
```

This will perform a release build of the extension. 

Read more about building your lambda extension in [the Cargo Lambda documentation](https://www.cargo-lambda.info/commands/build.html#extensions).

## Deploy testing

If you want to test a deployment, you can use the following command. By default it will publish as the layer name `rotel-extension-test`. 

```shell
make deploy
```

In order to use the layer in other AWS accounts, you will need to run the following command to publish it. Pass the ARN of the layer output from the above command as an argument.

```shell
 ./scripts/publish-lambda-version.sh arn:aws:lambda:us-east-1:999999999999:layer:rotel-extension-test:29
```

_You must set the valid AWS CLI credentials in your environment first._ 

## Production Deploy and Publish

When a release is created, the `release.yml` Github action will deploy and publish a new Lambda layer for both x86-64 and arm64 architectures.
The layer will be published to multiple regions, controlled by the regions matrix in the action script.

Release names/tags should follow a specific pattern:

For *alpha releases*:
- `v1-alpha`
- `v2-alpha`
- ...
  This will result in layers: `rotel-extension-amd64-alpha:1`, `rotel-extension-amd64-alpha:2`, etc. The last value is the Lambda layer version.

For production releases:
- `v1`
- `v2`
- ...
  This will result in layers: `rotel-extension:1`, `rotel-extension:2`, etc.

**NOTE**: There is no way to control the version number that AWS generates for a new Lambda layer. Therefore, we can only
rely on the auto-incrementing values to match the release name if we follow the same incrementing version scheme.

For the *arm64* architecture, the extension is named `rotel-extension-arm64-alpha` and `rotel-extension-arm64`.

_There may be some gaps in release numbers due to trying to keep version numbers and lambda layer version numbers in sync._

## Manual deploy

The Lambda layer version numbers can sometimes require manual adjustment to ensure they align across regions. They can
be incremented by manually deploying versions of the layer until the version matches the required level. Follow this process
to raise a layer version number to a specific value.

1. Pick a Rotel release build that you want to deploy, including the right architecture.
1. Find the Github action run for that tag, for example: [v1-alpha](https://github.com/streamfold/rotel-lambda-extension/actions/runs/14323997150).
1. Download the artifact you want to deploy to raise the layer version number.
1. Run: `rm -rf target/lambda && mkdir -p target/lambda && unzip extensions-<...>.zip -d target/lambda`
1. Login to the aws cli via sso
1. Run the following script:
```shell
./scripts/manual-deploy.sh <arch> <layer-name> <region> <how-many>
```
- `arch`: either _x86-64_ or _arm64_
- `layer-name`: full name of layer including arch and version suffix, examples: `rotel-extension-arm64-alpha`, `rotel-extension-amd64-alpha`, etc. (check Lambda console in case)
- `region`: region to deploy to
- `how-many`: how many times to deploy. If the current version is 3 and you need it to be 10, you'd pass "7" to deploy 7 times (3 + 7 = 10)
