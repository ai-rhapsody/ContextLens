/// Incremental Server-Sent Events decoder.
///
/// Network chunks can split a line, or even a multi-byte character, anywhere.
/// Bytes are buffered until a full `\n`-terminated line exists, and only
/// complete lines are decoded. Yields the payload of every `data:` line.
#[derive(Default)]
pub struct SseDecoder {
    buf: Vec<u8>,
}

impl SseDecoder {
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(end) = self.buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\n', '\r']);
            if let Some(rest) = line.strip_prefix("data:") {
                out.push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_data_lines_and_ignores_the_rest() {
        let mut d = SseDecoder::default();
        let got = d.push(b"event: message\ndata: {\"a\":1}\n\n: comment\ndata:[DONE]\n\n");
        assert_eq!(got, vec!["{\"a\":1}", "[DONE]"]);
    }

    #[test]
    fn handles_crlf() {
        let mut d = SseDecoder::default();
        assert_eq!(d.push(b"data: hi\r\n\r\n"), vec!["hi"]);
    }

    #[test]
    fn a_line_split_across_chunks_is_joined() {
        let mut d = SseDecoder::default();
        assert!(d.push(b"data: hel").is_empty());
        assert_eq!(d.push(b"lo\n"), vec!["hello"]);
    }

    #[test]
    fn a_multibyte_character_split_across_chunks_survives() {
        let bytes = "data: 你好\n".as_bytes();
        let cut = 8; // inside the three bytes of the character 你
        let mut d = SseDecoder::default();
        assert!(d.push(&bytes[..cut]).is_empty());
        assert_eq!(d.push(&bytes[cut..]), vec!["你好"]);
    }
}
