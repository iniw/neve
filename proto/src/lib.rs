pub mod server {
    tonic::include_proto!("neve.server.v1");
}

pub const AUTH_TOKEN_HEADER: &str = "auth-token";
