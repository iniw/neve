{
  lib,
  stdenv,
  fetchFromGitHub,
  makeWrapper,
  nodejs-slim,
  yarn-berry,
}:

stdenv.mkDerivation (finalAttrs: {
  pname = "protoc-gen-ts_proto";
  version = "2.12.1";

  src = fetchFromGitHub {
    owner = "stephenh";
    repo = "ts-proto";
    tag = "v${finalAttrs.version}";
    hash = "sha256-jxhvmf3W3jtEg1W785PsYq+qlrsrjPFKpBri1iDJN6w=";
  };

  patches = [ ./yarn-4.14.patch ];

  missingHashes = ./missing-hashes.json;

  offlineCache = yarn-berry.fetchYarnBerryDeps {
    inherit (finalAttrs) src patches missingHashes;
    hash = "sha256-UX9irnm9OG33CiMv4f5GDlk/XoxakY9dPBslNGLnP7s=";
  };

  nativeBuildInputs = [
    makeWrapper
    nodejs-slim
    yarn-berry
    yarn-berry.yarnBerryConfigHook
  ];

  buildPhase = ''
    runHook preBuild

    yarn build

    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall

    yarn workspaces focus --production

    mkdir -p "$out/lib/ts-proto"
    cp -R build node_modules package.json protoc-gen-ts_proto "$out/lib/ts-proto"

    makeWrapper '${lib.getExe nodejs-slim}' "$out/bin/protoc-gen-ts_proto" \
      --add-flags "$out/lib/ts-proto/protoc-gen-ts_proto"

    runHook postInstall
  '';

  meta = {
    description = "Protocol Buffers compiler plugin that generates TypeScript";
    homepage = "https://github.com/stephenh/ts-proto";
    license = lib.licenses.isc;
    mainProgram = "protoc-gen-ts_proto";
    platforms = lib.platforms.all;
  };
})
