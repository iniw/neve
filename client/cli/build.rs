use std::{io, path::PathBuf};

fn main() -> io::Result<()> {
    let proto = PathBuf::from("../../proto");
    let server = proto.join("neve/server/v1");

    tonic_prost_build::configure().compile_protos(
        &[server.join("auth.proto"), server.join("chat.proto")],
        &[proto],
    )
}
