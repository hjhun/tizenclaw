use serde_json::{json, Value};
use std::io::{ErrorKind, Read, Write};
use std::os::fd::FromRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

const MAX_IPC_MESSAGE_SIZE: usize = 10 * 1024 * 1024;
const DEFAULT_SOCKET_NAME: &str = "tizenclaw.sock";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Debug, Default)]
pub struct ClientOptions {
    pub socket_name: Option<String>,
    pub socket_path: Option<String>,
}

#[derive(Clone, Debug)]
pub struct IpcResponse {
    pub id: Option<Value>,
    pub result: Value,
    pub error: Option<Value>,
}

pub struct IpcClient {
    options: ClientOptions,
}

impl IpcClient {
    pub fn new(options: ClientOptions) -> Self {
        Self { options }
    }

    pub fn call(&self, method: &str, params: Value) -> Result<IpcResponse, String> {
        let mut stream = self.connect()?;
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        Self::write_frame(&mut stream, &request.to_string())?;

        loop {
            let frame = Self::read_frame(&mut stream)?;
            let payload: Value = serde_json::from_str(&frame)
                .map_err(|err| format!("Invalid JSON-RPC response: {}", err))?;

            if payload.get("method").and_then(Value::as_str) == Some("stream_chunk") {
                continue;
            }

            let error = payload.get("error").cloned();
            let result = payload.get("result").cloned().unwrap_or(Value::Null);
            let id = payload.get("id").cloned();
            return Ok(IpcResponse { id, result, error });
        }
    }

    fn connect(&self) -> Result<UnixStream, String> {
        if let Some(path) = self
            .options
            .socket_path
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            let stream = UnixStream::connect(Path::new(path))
                .map_err(|err| Self::format_connect_error(&format!("socket '{}'", path), &err))?;
            Self::configure_stream(&stream)?;
            return Ok(stream);
        }

        let socket_name = self
            .options
            .socket_name
            .clone()
            .or_else(|| std::env::var("TIZENCLAW_IPC_SOCKET_NAME").ok())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_SOCKET_NAME.to_string());

        if socket_name.starts_with('/') {
            let stream = UnixStream::connect(Path::new(&socket_name)).map_err(|err| {
                Self::format_connect_error(&format!("socket '{}'", socket_name), &err)
            })?;
            Self::configure_stream(&stream)?;
            return Ok(stream);
        }

        let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
        if fd < 0 {
            return Err(Self::format_connect_error(
                &format!("socket '@{}'", socket_name),
                &std::io::Error::last_os_error(),
            ));
        }

        let connect_result = unsafe {
            let mut addr: libc::sockaddr_un = std::mem::zeroed();
            addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
            for (index, byte) in socket_name.as_bytes().iter().enumerate() {
                addr.sun_path[index + 1] = *byte as libc::c_char;
            }
            let addr_len = (std::mem::size_of::<libc::sa_family_t>() + 1 + socket_name.len())
                as libc::socklen_t;
            libc::connect(fd, &addr as *const _ as *const libc::sockaddr, addr_len)
        };

        if connect_result < 0 {
            let error = std::io::Error::last_os_error();
            unsafe {
                libc::close(fd);
            }
            return Err(Self::format_connect_error(
                &format!("socket '@{}'", socket_name),
                &error,
            ));
        }

        let stream = unsafe { UnixStream::from_raw_fd(fd) };
        Self::configure_stream(&stream)?;
        Ok(stream)
    }

    fn configure_stream(stream: &UnixStream) -> Result<(), String> {
        stream
            .set_read_timeout(Some(DEFAULT_TIMEOUT))
            .map_err(|err| format!("Failed to set read timeout: {}", err))?;
        stream
            .set_write_timeout(Some(DEFAULT_TIMEOUT))
            .map_err(|err| format!("Failed to set write timeout: {}", err))
    }

    fn format_connect_error(target: &str, err: &std::io::Error) -> String {
        let guidance = match err.kind() {
            ErrorKind::NotFound | ErrorKind::ConnectionRefused | ErrorKind::TimedOut => {
                " Is the TizenClaw daemon running? Start it with ./deploy_host.sh and retry."
            }
            _ => ""
        };

        format!("Cannot connect to {}: {}.{}", target, err, guidance)
    }

    fn write_frame(stream: &mut UnixStream, payload: &str) -> Result<(), String> {
        let bytes = payload.as_bytes();
        if bytes.len() > MAX_IPC_MESSAGE_SIZE {
            return Err(format!(
                "Payload exceeds maximum IPC size: {} bytes",
                bytes.len()
            ));
        }

        let len = (bytes.len() as u32).to_be_bytes();
        stream
            .write_all(&len)
            .and_then(|_| stream.write_all(bytes))
            .map_err(|err| format!("Failed to write IPC frame: {}", err))
    }

    fn read_frame(stream: &mut UnixStream) -> Result<String, String> {
        let deadline = Instant::now() + DEFAULT_TIMEOUT;
        let mut len_buf = [0u8; 4];
        Self::read_exact_with_retry(stream, &mut len_buf, deadline, "read length")?;
        let payload_len = u32::from_be_bytes(len_buf) as usize;
        if payload_len == 0 || payload_len > MAX_IPC_MESSAGE_SIZE {
            return Err(format!("Invalid IPC payload size: {}", payload_len));
        }

        let mut payload = vec![0u8; payload_len];
        Self::read_exact_with_retry(stream, &mut payload, deadline, "read body")?;
        String::from_utf8(payload).map_err(|err| format!("Invalid UTF-8 IPC frame: {}", err))
    }

    fn read_exact_with_retry<R: Read>(
        reader: &mut R,
        buf: &mut [u8],
        deadline: Instant,
        context: &str,
    ) -> Result<(), String> {
        let mut offset = 0usize;
        while offset < buf.len() {
            match reader.read(&mut buf[offset..]) {
                Ok(0) => {
                    return Err(format!(
                        "IPC {} failed: unexpected EOF after {} of {} bytes",
                        context,
                        offset,
                        buf.len()
                    ));
                }
                Ok(read) => offset += read,
                Err(err)
                    if matches!(
                        err.kind(),
                        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                    ) && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(err) => return Err(format!("IPC {} failed: {}", context, err)),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::thread;
    use tempfile::tempdir;

    fn write_frame(stream: &mut UnixStream, payload: &str) {
        let bytes = payload.as_bytes();
        let len = (bytes.len() as u32).to_be_bytes();
        stream.write_all(&len).unwrap();
        stream.write_all(bytes).unwrap();
    }

    fn read_frame(stream: &mut UnixStream) -> String {
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).unwrap();
        let len = u32::from_be_bytes(len_buf) as usize;
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).unwrap();
        String::from_utf8(payload).unwrap()
    }

    #[test]
    fn call_reads_length_prefixed_jsonrpc_response() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("client-test.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_frame(&mut stream);
            let payload: Value = serde_json::from_str(&request).unwrap();
            assert_eq!(payload["method"], "ping");

            write_frame(
                &mut stream,
                &json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": { "pong": true }
                })
                .to_string(),
            );
        });

        let client = IpcClient::new(ClientOptions {
            socket_name: None,
            socket_path: Some(socket_path.display().to_string()),
        });
        let response = client.call("ping", json!({})).unwrap();
        assert_eq!(response.id, Some(json!(1)));
        assert_eq!(response.result, json!({ "pong": true }));
        assert_eq!(response.error, None);
        server.join().unwrap();
    }
}
