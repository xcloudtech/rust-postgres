use crate::config::SslMode;
use crate::maybe_tls_stream::MaybeTlsStream;
use crate::tls::private::ForcePrivateApi;
use crate::tls::TlsConnect;
use crate::Error;
use bytes::BytesMut;
use postgres_protocol::message::frontend;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub async fn connect_tls<S, T>(
    mut stream: S,
    mode: SslMode,
    tls: T,
    has_hostname: bool,
) -> Result<MaybeTlsStream<S, T::Stream>, Error>
where
    S: AsyncRead + AsyncWrite + Unpin,
    T: TlsConnect<S>,
{
    match mode {
        SslMode::Disable => return Ok(MaybeTlsStream::Raw(stream)),
        SslMode::Prefer if !tls.can_connect(ForcePrivateApi) => {
            return Ok(MaybeTlsStream::Raw(stream))
        }
        SslMode::Prefer | SslMode::Require | SslMode::VerifyCa | SslMode::VerifyFull => {}
    }

    let mut buf = BytesMut::new();
    frontend::ssl_request(&mut buf);
    stream.write_all(&buf).await.map_err(Error::io)?;

    let mut buf = [0];
    stream.read_exact(&mut buf).await.map_err(Error::io)?;

    if buf[0] != b'S' {
        match mode {
            SslMode::Require | SslMode::VerifyCa | SslMode::VerifyFull => {
                return Err(Error::tls("server does not support TLS".into()))
            }
            SslMode::Disable | SslMode::Prefer => return Ok(MaybeTlsStream::Raw(stream)),
        }
    }

    if !has_hostname {
        return Err(Error::tls("no hostname provided for TLS handshake".into()));
    }

    let stream = tls
        .connect(stream)
        .await
        .map_err(|e| Error::tls(e.into()))?;

    Ok(MaybeTlsStream::Tls(stream))
}
