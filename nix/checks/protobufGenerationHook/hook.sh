protobufGenerationHook() {
  echo "generating Protobuf bindings"

  # buf defaults this to ~/.cache, so it fails under the nix sandbox because $HOME isn't set.
  # Leaving it in the default value fails with:
  #
  # mkdir /homeless-shelter: operation not permitted
  #
  # See <https://buf.build/docs/bsr/ci-cd/setup/#cache-module-downloads>
  export BUF_CACHE_DIR="$TMPDIR/.buf-cache"

  buf generate
}

preBuildHooks+=(protobufGenerationHook)
