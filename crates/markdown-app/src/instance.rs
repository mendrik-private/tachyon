use std::{
    fs,
    io::{ErrorKind, Write as _},
    os::unix::{
        fs::FileTypeExt as _,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use serde::{Deserialize, Serialize};

const MODE_ENV: &str = "MINERAL_INSTANCE_MODE";
const SOCKET_ENV: &str = "MINERAL_INSTANCE_SOCKET";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum LaunchMode {
    Standalone,
    Server(PathBuf),
    Client(PathBuf),
}

impl LaunchMode {
    pub(super) fn from_env() -> Result<Self, String> {
        let Some(mode) = std::env::var_os(MODE_ENV) else {
            return Ok(Self::Standalone);
        };
        let mode = mode
            .into_string()
            .map_err(|_| format!("{MODE_ENV} must be valid Unicode"))?;
        let socket = std::env::var_os(SOCKET_ENV)
            .map(PathBuf::from)
            .ok_or_else(|| format!("{SOCKET_ENV} is required when {MODE_ENV} is set"))?;
        match mode.as_str() {
            "server" => Ok(Self::Server(socket)),
            "client" => Ok(Self::Client(socket)),
            _ => Err(format!("{MODE_ENV} must be `server` or `client`")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct StartupRequest {
    pub(super) output: PathBuf,
    pub(super) label: String,
    pub(super) cache_state: String,
    pub(super) started_unix_nanos: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) enum Request {
    Open {
        path: PathBuf,
        startup: Option<StartupRequest>,
    },
    Shutdown,
}

#[derive(Debug, Deserialize, Serialize)]
struct Response {
    error: Option<String>,
}

pub(super) fn forward(socket: &Path, request: &Request) -> Result<(), String> {
    let mut stream = UnixStream::connect(socket)
        .map_err(|error| format!("could not connect to {}: {error}", socket.display()))?;
    serde_json::to_writer(&mut stream, request)
        .map_err(|error| format!("could not encode instance request: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("could not send instance request: {error}"))?;
    stream
        .shutdown(std::net::Shutdown::Write)
        .map_err(|error| format!("could not finish instance request: {error}"))?;
    let response: Response = serde_json::from_reader(stream)
        .map_err(|error| format!("could not read instance response: {error}"))?;
    match response.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

pub(super) struct Inbound {
    pub(super) request: Request,
    completion: Completion,
}

impl Inbound {
    pub(super) fn into_parts(self) -> (Request, Completion) {
        (self.request, self.completion)
    }
}

pub(super) struct Completion {
    response: Option<mpsc::SyncSender<Response>>,
}

impl Completion {
    pub(super) fn finish(mut self, result: Result<(), String>) {
        if let Some(response) = self.response.take() {
            let _ = response.send(Response {
                error: result.err(),
            });
        }
    }
}

impl Drop for Completion {
    fn drop(&mut self) {
        if let Some(response) = self.response.take() {
            let _ = response.send(Response {
                error: Some("instance request ended before completion".into()),
            });
        }
    }
}

pub(super) struct Server {
    receiver: UnboundedReceiver<Inbound>,
    guard: ServerGuard,
}

impl Server {
    pub(super) fn bind(socket: PathBuf) -> Result<Self, String> {
        let listener = bind_listener(&socket)?;
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("could not configure {}: {error}", socket.display()))?;
        let (sender, receiver) = unbounded();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread_socket = socket.clone();
        let thread = thread::Builder::new()
            .name("mineral-instance-listener".into())
            .spawn(move || listen(listener, sender, thread_stop, &thread_socket))
            .map_err(|error| format!("could not start instance listener: {error}"))?;
        Ok(Self {
            receiver,
            guard: ServerGuard {
                socket,
                stop,
                thread: Some(thread),
            },
        })
    }

    pub(super) fn into_parts(self) -> (UnboundedReceiver<Inbound>, ServerGuard) {
        (self.receiver, self.guard)
    }
}

pub(super) struct ServerGuard {
    socket: PathBuf,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = fs::remove_file(&self.socket);
    }
}

fn bind_listener(socket: &Path) -> Result<UnixListener, String> {
    if let Some(parent) = socket
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "could not create instance socket directory {}: {error}",
                parent.display()
            )
        })?;
    }
    match UnixListener::bind(socket) {
        Ok(listener) => Ok(listener),
        Err(error) if error.kind() == ErrorKind::AddrInUse => {
            if UnixStream::connect(socket).is_ok() {
                return Err(format!(
                    "an instance server is already listening at {}",
                    socket.display()
                ));
            }
            let metadata = fs::symlink_metadata(socket).map_err(|metadata_error| {
                format!(
                    "could not inspect stale instance socket {}: {metadata_error}",
                    socket.display()
                )
            })?;
            if !metadata.file_type().is_socket() {
                return Err(format!(
                    "refusing to replace non-socket path {}",
                    socket.display()
                ));
            }
            fs::remove_file(socket).map_err(|remove_error| {
                format!(
                    "could not remove stale instance socket {}: {remove_error}",
                    socket.display()
                )
            })?;
            UnixListener::bind(socket)
                .map_err(|bind_error| format!("could not bind {}: {bind_error}", socket.display()))
        }
        Err(error) => Err(format!("could not bind {}: {error}", socket.display())),
    }
}

fn listen(
    listener: UnixListener,
    sender: UnboundedSender<Inbound>,
    stop: Arc<AtomicBool>,
    socket: &Path,
) {
    while !stop.load(Ordering::Acquire) {
        let mut stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2));
                continue;
            }
            Err(error) => {
                eprintln!("instance listener {} failed: {error}", socket.display());
                break;
            }
        };
        let request = match serde_json::from_reader(&mut stream) {
            Ok(request) => request,
            Err(error) => {
                let _ = serde_json::to_writer(
                    stream,
                    &Response {
                        error: Some(format!("invalid instance request: {error}")),
                    },
                );
                continue;
            }
        };
        let (response_sender, response_receiver) = mpsc::sync_channel(1);
        if sender
            .unbounded_send(Inbound {
                request,
                completion: Completion {
                    response: Some(response_sender),
                },
            })
            .is_err()
        {
            let _ = serde_json::to_writer(
                stream,
                &Response {
                    error: Some("instance server is shutting down".into()),
                },
            );
            break;
        }
        let response = response_receiver.recv().unwrap_or_else(|_| Response {
            error: Some("instance server dropped the response".into()),
        });
        let _ = serde_json::to_writer(stream, &response);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt as _;

    fn isolated_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mineral-instance-{label}-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    #[test]
    fn protocol_round_trips_paths_and_startup_metadata() {
        let request = Request::Open {
            path: PathBuf::from("/tmp/notes/example.md"),
            startup: Some(StartupRequest {
                output: PathBuf::from("/tmp/report.json"),
                label: "warm-01".into(),
                cache_state: "warm".into(),
                started_unix_nanos: 42,
            }),
        };
        let encoded = serde_json::to_vec(&request).expect("encode request");
        let decoded: Request = serde_json::from_slice(&encoded).expect("decode request");
        assert!(matches!(
            decoded,
            Request::Open {
                path,
                startup: Some(StartupRequest { started_unix_nanos: 42, .. })
            } if path == Path::new("/tmp/notes/example.md")
        ));
    }

    #[test]
    fn server_forwards_one_request_and_cleans_up_its_socket() {
        let socket = isolated_path("round-trip");
        let server = Server::bind(socket.clone()).expect("bind server");
        let (mut receiver, guard) = server.into_parts();
        let client_socket = socket.clone();
        let client = thread::spawn(move || forward(&client_socket, &Request::Shutdown));
        let inbound = futures::executor::block_on(receiver.next()).expect("request");
        let (request, completion) = inbound.into_parts();
        assert!(matches!(request, Request::Shutdown));
        completion.finish(Ok(()));
        assert!(client.join().expect("client thread").is_ok());
        drop(receiver);
        drop(guard);
        assert!(!socket.exists());
    }

    #[test]
    fn server_refuses_to_replace_a_non_socket_path() {
        let socket = isolated_path("file");
        fs::write(&socket, "keep me").expect("fixture");
        let error = match Server::bind(socket.clone()) {
            Ok(_) => panic!("regular file must not be replaced"),
            Err(error) => error,
        };
        assert!(error.contains("refusing to replace non-socket"));
        assert_eq!(
            fs::read_to_string(&socket).expect("fixture remains"),
            "keep me"
        );
        fs::remove_file(socket).expect("cleanup fixture");
    }
}
