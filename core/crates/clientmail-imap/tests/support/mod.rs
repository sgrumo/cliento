//! A scripted IMAP server that plays a transcript over an in-memory pipe.
//!
//! Integration tests compile each file in `tests/` as its own crate, so `pub` here only means
//! "visible to the test that includes this module".
#![expect(
    unreachable_pub,
    reason = "a module shared by test crates; `pub` is its interface to them"
)]
//!
//! A transcript is a text file of lines:
//!
//! - `S: <line>` is sent to the client as is, followed by CRLF. `<ff>` stands for the byte 0xFF.
//! - `C: <line>` must be the client's next line. `<xoauth2>` stands for the expected base64 SASL
//!   string.
//! - `# ...` is a comment.
//! - `CLOSE` as the last line closes the connection at once; `HOLD` keeps it open and silent.
//!   Without either, the server waits for the client to close and fails if it sends anything
//!   more.

use std::path::PathBuf;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

pub const USER: &str = "ada@gmail.com";
pub const TOKEN: &str = "ya29.test-token";

enum Line {
    Server(String),
    Client(String),
}

enum End {
    ExpectClose,
    Close,
    Hold,
}

pub struct Script {
    lines: Vec<Line>,
    end: End,
}

impl Script {
    /// Loads `tests/transcripts/<name>.txt`.
    #[expect(
        clippy::unwrap_used,
        reason = "test support: a missing transcript is a failing test"
    )]
    pub fn load(name: &str) -> Self {
        let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "transcripts", name]
            .iter()
            .collect();
        let text = std::fs::read_to_string(path.with_extension("txt")).unwrap();
        let mut lines = Vec::new();
        let mut end = End::ExpectClose;
        for line in text.lines() {
            if let Some(server) = line.strip_prefix("S: ") {
                lines.push(Line::Server(server.to_owned()));
            } else if let Some(client) = line.strip_prefix("C: ") {
                lines.push(Line::Client(client.to_owned()));
            } else if line == "C:" {
                lines.push(Line::Client(String::new()));
            } else if line == "CLOSE" {
                end = End::Close;
            } else if line == "HOLD" {
                end = End::Hold;
            } else {
                assert!(line.is_empty() || line.starts_with('#'), "bad line: {line}");
            }
        }
        Self { lines, end }
    }

    /// Plays the script on `stream`. Returns the first difference, if any.
    pub async fn play(self, stream: DuplexStream) -> Result<(), String> {
        let (read, mut write) = tokio::io::split(stream);
        let mut read = BufReader::new(read);
        let xoauth2 =
            STANDARD.encode(format!("user={USER}\u{1}auth=Bearer {TOKEN}\u{1}\u{1}").as_bytes());
        for line in self.lines {
            match line {
                Line::Server(text) => {
                    let mut bytes = Vec::with_capacity(text.len() + 2);
                    for (index, part) in text.split("<ff>").enumerate() {
                        if index > 0 {
                            bytes.push(0xFF);
                        }
                        bytes.extend_from_slice(part.as_bytes());
                    }
                    bytes.extend_from_slice(b"\r\n");
                    write
                        .write_all(&bytes)
                        .await
                        .map_err(|error| error.to_string())?;
                    write.flush().await.map_err(|error| error.to_string())?;
                }
                Line::Client(expected) => {
                    let expected = expected.replace("<xoauth2>", &xoauth2);
                    let mut received = String::new();
                    read.read_line(&mut received)
                        .await
                        .map_err(|error| error.to_string())?;
                    // End of file, or a line without CRLF, is never what a script expects.
                    let Some(received) = received.strip_suffix("\r\n") else {
                        return Err(format!(
                            "expected {expected:?}, got end of input {received:?}"
                        ));
                    };
                    if received != expected {
                        return Err(format!("expected {expected:?}, got {received:?}"));
                    }
                }
            }
        }
        match self.end {
            End::Close => Ok(()),
            End::Hold => {
                // Keep both halves alive and say nothing.
                let _halves = (read, write);
                std::future::pending::<()>().await;
                Ok(())
            }
            End::ExpectClose => {
                let mut extra = String::new();
                let read_bytes = read.read_line(&mut extra).await.unwrap_or(0);
                if read_bytes == 0 {
                    Ok(())
                } else {
                    Err(format!("unexpected line from the client: {extra:?}"))
                }
            }
        }
    }
}
