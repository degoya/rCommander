//! An open data connection of a [`Connection`]: plain or TLS, read or written, then finished
//! with the server's completion reply.

use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use suppaftp::FtpResult;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpStream,
};
use tokio_rustls::client::TlsStream;

use super::Connection;
use crate::ftp::active::close_data;

/// An open data connection: read a download, write an upload, then [`Transfer::finish`].
pub(crate) struct Transfer {
    /// The command it carries the data of (`RETR`, `STOR`, `APPE`, `MLSD`, `LIST`), whose
    /// completion reply ends it.
    pub command: &'static str,
    pub(super) data: Data,
}

pub(super) enum Data {
    Plain(TcpStream),
    Secure(Box<TlsStream<TcpStream>>),
}

impl Transfer {
    /// Closes the data connection and reads the server's completion reply on `connection`, the
    /// control connection that opened it. It is closed with `close_notify` (TLS) and drained
    /// first (`close_data`), so no unread session ticket turns the close into a reset.
    pub async fn finish(mut self, connection: &mut Connection) -> FtpResult<()> {
        let closed = close_data(&mut self).await;
        drop(self);
        let reply = connection.transfer_reply().await;
        closed.map_err(suppaftp::FtpError::ConnectionError)?;
        reply
    }
}

impl AsyncRead for Transfer {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match &mut self.get_mut().data {
            Data::Plain(stream) => Pin::new(stream).poll_read(context, buffer),
            Data::Secure(stream) => Pin::new(stream.as_mut()).poll_read(context, buffer),
        }
    }
}

impl AsyncWrite for Transfer {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        match &mut self.get_mut().data {
            Data::Plain(stream) => Pin::new(stream).poll_write(context, bytes),
            Data::Secure(stream) => Pin::new(stream.as_mut()).poll_write(context, bytes),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.get_mut().data {
            Data::Plain(stream) => Pin::new(stream).poll_flush(context),
            Data::Secure(stream) => Pin::new(stream.as_mut()).poll_flush(context),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.get_mut().data {
            Data::Plain(stream) => Pin::new(stream).poll_shutdown(context),
            Data::Secure(stream) => Pin::new(stream.as_mut()).poll_shutdown(context),
        }
    }
}
