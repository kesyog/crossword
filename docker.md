# Docker + Cloud Run

Documenting my janky workflow.

Image name: `us.gcr.io/<project id>/<package name>`

```sh
# Refresh credentials
gcloud auth login

# Building an image
# On Linux
docker build -t `<image name>` .
# On Apple
docker buildx build --platform linux/amd64 -t <image name> .

# (optional) Open a shell in container so you can run commands and test
docker run -it --rm <image name>:latest bash

# Push image to registry
docker push <image name>:latest

# Use Cloud console to point Cloud Run to latest image
```
