use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use neve_protocol::{client, server};
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::{
        TcpListener,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    select,
    sync::mpsc::{self, Sender},
    task,
};

enum TaskEvent {
    Message {
        client: SocketAddr,
        name: Option<String>,
        data: String,
    },
    Close {
        client: SocketAddr,
    },
}

async fn handle_client(
    mut stream: OwnedReadHalf,
    addr: SocketAddr,
    sender: Sender<TaskEvent>,
) -> io::Result<()> {
    let mut registered_name = None;

    loop {
        let mut buffer = [0; 4096];
        let n = match stream.read(&mut buffer).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(err) => {
                dbg!(err);
                continue;
            }
        };

        let bytes = &buffer[..n];
        let event = client::access(bytes);
        match event {
            client::ArchivedEvent::Message { data } => {
                _ = sender
                    .send(TaskEvent::Message {
                        client: addr,
                        name: registered_name.clone(),
                        data: data.to_string(),
                    })
                    .await;
            }
            client::ArchivedEvent::Register { name } => {
                registered_name = Some(name.to_string());
            }
        };
    }

    _ = sender.send(TaskEvent::Close { client: addr }).await;

    Ok(())
}

struct Connection {
    tx: OwnedWriteHalf,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener =
        TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 5547)).await?;

    let (sender, mut receiver) = mpsc::channel(64);

    let mut connections = HashMap::new();

    loop {
        select! {
            accept = listener.accept() => match accept {
                Ok((stream, addr)) => {
                    let (rx, tx) = stream.into_split();
                    connections.insert(addr, Connection {
                        tx,
                    });
                    _ = task::spawn(handle_client(rx, addr, sender.clone()))
                }
                Err(err) => _ = dbg!(err),
            },
            Some(msg) = receiver.recv() => match msg {
                TaskEvent::Message { client, name, data } => {
                    let from = name.unwrap_or_else(|| "unknown".to_owned());
                    for (_, conn) in connections.iter_mut().filter(|(addr, _)| **addr != client) {
                        let bytes = server::to_bytes(server::Event::Message { from: from.clone(), data: data.clone() });
                        _ = conn.tx.write_all(&bytes).await;
                    }
                }
                TaskEvent::Close { client } => {
                    connections.remove(&client);
                }
            }
        }
    }
}
