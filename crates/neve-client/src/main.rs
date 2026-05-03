use std::{
    env::args,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use tokio::{
    io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, stdin},
    net::TcpStream,
    select,
};

use neve_protocol::{client, server};

#[tokio::main]
async fn main() -> io::Result<()> {
    let Some(name) = args().nth(1) else {
        eprintln!("Usage: neve-client [username]");
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    };

    let mut stream =
        TcpStream::connect(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 5547)).await?;

    let bytes = client::to_bytes(client::Event::Register { name });
    stream.write_all(&bytes).await?;

    let mut stdin = BufReader::new(stdin());

    let mut buf = [0; 4096];

    loop {
        let mut line = String::new();
        select! {
            read = stdin.read_line(&mut line) => match read {
                Ok(0) => break,
                Ok(_) => {
                    // Strip the newline
                    line.pop();

                    let bytes = client::to_bytes(client::Event::Message { data: line.clone() });
                    stream.write_all(&bytes).await?;
                }
                Err(err) => {
                    dbg!(err);
                }
            },
            read = stream.read(&mut buf) => match read {
                Ok(0) => break,
                Ok(n) => {
                    let bytes = &buf[..n];
                    let event = server::access(bytes);
                    match event {
                        server::ArchivedEvent::Message { from, data } => {
                            println!("{}: {}", from, data);
                        }
                    }
                }
                Err(err) =>  {
                    dbg!(err);
                },
            }
        }
    }

    Ok(())
}
