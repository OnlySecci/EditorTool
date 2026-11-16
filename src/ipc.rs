//! File-open requests for an already running instance. Listener blocks while idle.
use crate::{Action, Message};
use fltk::app;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};
#[derive(Serialize, Deserialize)]
struct Endpoint {
    #[serde(default)]
    protocol: u16,
    port: u16,
    token: String,
}
#[derive(Serialize, Deserialize)]
struct Request {
    #[serde(default)]
    id: String,
    token: String,
    paths: Vec<PathBuf>,
}
pub fn listen(directory: &Path, tx: app::Sender<Message>) -> Result<(), String> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
    let endpoint = Endpoint {
        protocol: 156,
        port: listener.local_addr().map_err(|e| e.to_string())?.port(),
        token: crate::private_fs::random_token().map_err(|e| e.to_string())?,
    };
    crate::session::atomic_write(
        &directory.join("instance.json"),
        &serde_json::to_vec(&endpoint).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    thread::spawn(move || {
        for stream in listener.incoming() {
            if let Ok(mut stream) = stream {
                let started = std::time::Instant::now();
                let mut bytes = Vec::new();
                let received = (|| -> std::io::Result<()> {
                    let mut buffer = [0u8; 4096];
                    loop {
                        let remaining = Duration::from_secs(2)
                            .checked_sub(started.elapsed())
                            .filter(|d| !d.is_zero())
                            .ok_or_else(|| {
                                std::io::Error::new(std::io::ErrorKind::TimedOut, "IPC-Zeitlimit")
                            })?;
                        stream.set_read_timeout(Some(remaining))?;
                        let n = stream.read(&mut buffer)?;
                        if n == 0 {
                            return Ok(());
                        }
                        if bytes.len() + n > 256 * 1024 {
                            return Err(std::io::Error::other("IPC size limit"));
                        }
                        bytes.extend_from_slice(&buffer[..n]);
                    }
                })();
                if received.is_ok() {
                    if let Ok(request) = serde_json::from_slice::<Request>(&bytes) {
                        if request.token == endpoint.token && request.paths.len() <= 64 && request.id.len()<=64 && request.id.bytes().all(|c|c.is_ascii_hexdigit()) {
                            let (confirmed, reply) = std::sync::mpsc::channel();
                            let id=if request.id.is_empty() {crate::private_fs::random_token().unwrap_or_default()}else{request.id};
                            tx.send((0, Action::IpcOpen(id,request.paths, confirmed)));
                            if reply.recv_timeout(Duration::from_secs(3)).is_ok() {
                                let _ = stream.write_all(b"ET156OK");
                            }
                        }
                    }
                }
            }
        }
    });
    Ok(())
}
pub fn forward(directory: &Path, paths: Vec<PathBuf>) -> Result<(), String> {
    static ID: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    let id=ID.get_or_init(||crate::private_fs::random_token().ok()).as_ref().ok_or("Could not create handoff identifier")?.clone();
    let endpoint: Endpoint = serde_json::from_slice(
        &crate::limits::read(&directory.join("instance.json"), 4096).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], endpoint.port));
    let mut stream =
        TcpStream::connect_timeout(&address, Duration::from_secs(1)).map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    stream
        .write_all(
            &serde_json::to_vec(&Request {
                id,
                token: endpoint.token,
                paths,
            })
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    if endpoint.protocol < 156 {
        return Ok(());
    }
    stream
        .shutdown(std::net::Shutdown::Write)
        .map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(4)))
        .map_err(|e| e.to_string())?;
    let mut confirmation = [0u8; 7];
    stream
        .read_exact(&mut confirmation)
        .map_err(|e| e.to_string())?;
    if &confirmation != b"ET156OK" {
        return Err("Existing instance did not confirm reopening".into());
    }
    Ok(())
}
