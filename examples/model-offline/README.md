# Offline model container check

Use the Dockerfile with a prepared build directory containing:

- `wheels/`: the qualified Linux Core/worker wheels and their complete CPU
  dependency wheelhouse, downloaded explicitly for the intended image platform;
- `bundle/`: the verified GLiNER2 bundle from the provisioner;
- `session.json`: the bundle's manifest digest, `/opt/datafog/bundle`, the image's
  absolute Python interpreter path, qualified GLiNER2 settings and caller limits.

Pass an approved pinned Python 3.12 Linux image digest (including the native
libraries required by its qualified PyTorch wheels), then verify offline startup:

```sh
docker build --network=none --build-arg PYTHON_IMAGE=python@sha256:<approved-digest> \
  -f /path/to/datafog-core/examples/model-offline/Dockerfile -t datafog-model-beta /path/to/prepared-context
docker run --rm --network=none datafog-model-beta
```

The build installs solely from the supplied wheelhouse. The runtime opens,
warms, inspects and closes through Core without downloading assets. No server or
application UI is included. Retain the wheelhouse's package licenses and the
model card. Build and run this on the intended Linux target; having this example
alone is not container qualification evidence.
