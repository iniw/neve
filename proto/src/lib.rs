pub mod server {
    pub mod v1 {
        include!("generated/neve/server/v1/neve.server.v1.rs");
    }
}

pub const AUTH_TOKEN_HEADER: &str = "neve-auth-token";
