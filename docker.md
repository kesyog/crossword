# Docker + Cloud Run

Documenting my janky workflow.

Image name: `us.gcr.io/<project id>/<image name>`

```sh
# Refresh credentials
gcloud auth login

# Building an image
docker build -t `<image name>` .

# Open a shell in container so you can run commands and test
docker run -it --rm us.gcr.io/xword-stats/xword:latest bash

# Push image to registry
docker push us.gcr.io/xword-stats/xword:latest

# Use Cloud console to point Cloud Run to latest image
```
