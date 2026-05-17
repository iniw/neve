use std::io;

fn main() -> io::Result<()> {
    tonic_prost_build::compile_protos("./proto/neve/v1/neve.proto")
}
