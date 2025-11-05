pushd src
# create a builder if you don't have one
# docker buildx create --use
docker buildx build \
  --platform linux/amd64,linux/arm64 \
  -t hhanh00/zcash-vote-docker:1.2.1 \
  --push .
popd
