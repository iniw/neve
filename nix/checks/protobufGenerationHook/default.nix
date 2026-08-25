{
  makeSetupHook,
  buf,
  protoc-gen-prost,
  protoc-gen-tonic,
  protoc-gen-ts_proto,
}:
makeSetupHook {
  name = "protobuf-generation-hook";
  propagatedBuildInputs = [
    buf
    protoc-gen-prost
    protoc-gen-tonic
    protoc-gen-ts_proto
  ];
} ./hook.sh
