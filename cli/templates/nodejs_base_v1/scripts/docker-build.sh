#!/bin/bash

# Basic Docker build script
# Uses placeholders substituted by spawnpoint

# Exit immediately if a command exits with a non-zero status.
set -e

# Define image name using placeholders
# Example: my-org/my-cool-app:latest
# Note: Needs orgScope and kebabProjectName variables defined and substituted.
# We might need to pass these as args or env vars if not directly substituting here.
# For simplicity, let's use the kebab name directly.
IMAGE_NAME="--kebab-project-name--"
IMAGE_TAG="latest"
FULL_IMAGE_NAME="${IMAGE_NAME}:${IMAGE_TAG}"

# Optionally add org scope if provided (requires more complex logic or passing vars)
# ORG_SCOPE="--org-scope--" # This is the raw placeholder
# if [[ -n "$ORG_SCOPE" && "$ORG_SCOPE" != "--org-scope--" && "$USE_ORG_SCOPE_VAR_FROM_ENV_OR_ARG" == "true" ]]; then
#   FULL_IMAGE_NAME="${ORG_SCOPE}/${IMAGE_NAME}:${IMAGE_TAG}"
# fi

echo "Building Docker image: ${FULL_IMAGE_NAME}..."

# Run the Docker build command
# Use --no-cache for clean builds if needed
# Pass build arguments if your Dockerfile needs them
docker build --platform linux/amd64 -t "${FULL_IMAGE_NAME}" .

echo "Docker image built successfully: ${FULL_IMAGE_NAME}"

# Optionally push the image
# echo "Pushing image..."
# docker push "${FULL_IMAGE_NAME}"
# echo "Image pushed."