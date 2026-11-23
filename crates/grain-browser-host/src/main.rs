//! Grain Pilot native messaging host.
//!
//! The browser starts this process when the Grain Pilot extension connects
//! (`chrome.runtime.connectNative`) and talks to it over stdio: each message
//! is a 4-byte native-endian length followed by that many bytes of JSON.
//! The host relays messages unchanged to Grain's named pipe, in the same
//! framing, and back. It never parses them: Grain checks every message.
//!
//! It exits when either side closes, so it lives only as long as the
//! extension's connection and Grain both do. When Grain is not running it
//! exits at once and the extension tries again later.

#[cfg(any(windows, test))]
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Largest message accepted from either side. Browsers cap host → extension
/// messages at 1 MB; extension → host messages are page reads, kept well
/// below this.
#[cfg(any(windows, test))]
const MAX_MESSAGE: u32 = 8 * 1024 * 1024;

/// Read one framed message; `None` at a clean end of stream.
#[cfg(any(windows, test))]
async fn read_frame<R: AsyncReadExt + Unpin>(reader: &mut R) -> std::io::Result<Option<Vec<u8>>> {
    let mut length = [0u8; 4];
    match reader.read_exact(&mut length).await {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let length = u32::from_ne_bytes(length);
    if length > MAX_MESSAGE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "message too large",
        ));
    }
    let mut body = vec![0u8; length as usize];
    reader.read_exact(&mut body).await?;
    Ok(Some(body))
}

#[cfg(any(windows, test))]
async fn write_frame<W: AsyncWriteExt + Unpin>(writer: &mut W, body: &[u8]) -> std::io::Result<()> {
    writer.write_all(&(body.len() as u32).to_ne_bytes()).await?;
    writer.write_all(body).await?;
    writer.flush().await
}

#[cfg(windows)]
async fn relay<R, W>(mut from: R, mut to: W) -> std::io::Result<()>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    while let Some(body) = read_frame(&mut from).await? {
        write_frame(&mut to, &body).await?;
    }
    Ok(())
}

/// Grain's pipe for this Windows user.
#[cfg(windows)]
fn pipe_name() -> String {
    let user: String = std::env::var("USERNAME")
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    format!(r"\\.\pipe\grain-pilot-{user}")
}

#[cfg(windows)]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    use tokio::net::windows::named_pipe::ClientOptions;
    let pipe = match ClientOptions::new().open(pipe_name()) {
        Ok(pipe) => pipe,
        // Grain is not running or Pilot is off. Exiting closes the
        // extension's port; it tries again on its next alarm.
        Err(_) => std::process::exit(1),
    };
    let (pipe_read, pipe_write) = tokio::io::split(pipe);
    let to_grain = relay(tokio::io::stdin(), pipe_write);
    let to_browser = relay(pipe_read, tokio::io::stdout());
    // Either side closing ends the host.
    tokio::select! {
        _ = to_grain => {}
        _ = to_browser => {}
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("grain-browser-host runs on Windows only for now");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn frames_round_trip_and_end_cleanly() {
        let mut buffer = Vec::new();
        write_frame(&mut buffer, br#"{"id":1}"#).await.unwrap();
        write_frame(&mut buffer, b"{}").await.unwrap();
        let mut reader = &buffer[..];
        assert_eq!(
            read_frame(&mut reader).await.unwrap().as_deref(),
            Some(&br#"{"id":1}"#[..])
        );
        assert_eq!(
            read_frame(&mut reader).await.unwrap().as_deref(),
            Some(&b"{}"[..])
        );
        assert_eq!(read_frame(&mut reader).await.unwrap(), None);
    }

    #[tokio::test]
    async fn oversized_frames_are_refused() {
        let mut buffer = (MAX_MESSAGE + 1).to_ne_bytes().to_vec();
        buffer.extend_from_slice(b"x");
        let mut reader = &buffer[..];
        assert!(read_frame(&mut reader).await.is_err());
    }
}
