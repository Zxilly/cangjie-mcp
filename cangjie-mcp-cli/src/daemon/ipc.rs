use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

// ── Macros for AsyncRead/AsyncWrite delegation ─────────────────────────────

macro_rules! impl_async_io {
    ($stream_type:ident, $( $variant:ident ),+) => {
        impl AsyncRead for $stream_type {
            fn poll_read(
                self: Pin<&mut Self>,
                cx: &mut Context<'_>,
                buf: &mut ReadBuf<'_>,
            ) -> Poll<io::Result<()>> {
                match self.get_mut() {
                    $( $stream_type::$variant(s) => Pin::new(s).poll_read(cx, buf), )+
                }
            }
        }

        impl AsyncWrite for $stream_type {
            fn poll_write(
                self: Pin<&mut Self>,
                cx: &mut Context<'_>,
                buf: &[u8],
            ) -> Poll<io::Result<usize>> {
                match self.get_mut() {
                    $( $stream_type::$variant(s) => Pin::new(s).poll_write(cx, buf), )+
                }
            }

            fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
                match self.get_mut() {
                    $( $stream_type::$variant(s) => Pin::new(s).poll_flush(cx), )+
                }
            }

            fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
                match self.get_mut() {
                    $( $stream_type::$variant(s) => Pin::new(s).poll_shutdown(cx), )+
                }
            }
        }
    };
}

// ── Client-side stream ─────────────────────────────────────────────────────

pub enum IpcStream {
    #[cfg(unix)]
    Unix(tokio::net::UnixStream),
    #[cfg(windows)]
    Pipe(tokio::net::windows::named_pipe::NamedPipeClient),
}

#[cfg(unix)]
impl_async_io!(IpcStream, Unix);
#[cfg(windows)]
impl_async_io!(IpcStream, Pipe);

// ── Server-side stream ─────────────────────────────────────────────────────

pub enum ServerIpcStream {
    #[cfg(unix)]
    Unix(tokio::net::UnixStream),
    #[cfg(windows)]
    Pipe(tokio::net::windows::named_pipe::NamedPipeServer),
}

#[cfg(unix)]
impl_async_io!(ServerIpcStream, Unix);
#[cfg(windows)]
impl_async_io!(ServerIpcStream, Pipe);

// ── Listener ───────────────────────────────────────────────────────────────

pub struct IpcListener {
    #[cfg(unix)]
    inner: tokio::net::UnixListener,
    #[cfg(windows)]
    pipe_name: String,
    #[cfg(windows)]
    current_server: tokio::net::windows::named_pipe::NamedPipeServer,
}

impl IpcListener {
    pub fn bind() -> io::Result<Self> {
        super::paths::ensure_runtime_dir()?;

        #[cfg(unix)]
        {
            let path = super::paths::socket_path();
            let _ = std::fs::remove_file(&path);
            let listener = tokio::net::UnixListener::bind(&path)?;
            Ok(Self { inner: listener })
        }

        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ServerOptions;
            let pipe_name = super::paths::pipe_name();
            let server = ServerOptions::new()
                .first_pipe_instance(true)
                .create(&pipe_name)?;
            Ok(Self {
                pipe_name,
                current_server: server,
            })
        }
    }

    pub async fn accept(&mut self) -> io::Result<ServerIpcStream> {
        #[cfg(unix)]
        {
            let (stream, _) = self.inner.accept().await?;
            Ok(ServerIpcStream::Unix(stream))
        }

        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ServerOptions;
            self.current_server.connect().await?;
            let connected = {
                let new_server = ServerOptions::new().create(&self.pipe_name)?;
                std::mem::replace(&mut self.current_server, new_server)
            };
            Ok(ServerIpcStream::Pipe(connected))
        }
    }
}

// ── Client connect ─────────────────────────────────────────────────────────

pub async fn ipc_connect() -> io::Result<IpcStream> {
    #[cfg(unix)]
    {
        let path = super::paths::socket_path();
        let stream = tokio::net::UnixStream::connect(&path).await?;
        Ok(IpcStream::Unix(stream))
    }

    #[cfg(windows)]
    {
        let pipe_name = super::paths::pipe_name();
        let client = tokio::net::windows::named_pipe::ClientOptions::new().open(&pipe_name)?;
        Ok(IpcStream::Pipe(client))
    }
}
