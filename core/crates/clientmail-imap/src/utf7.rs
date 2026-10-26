use base64::Engine as _;
use base64::engine::general_purpose::STANDARD_NO_PAD;

/// Decodes a folder name from IMAP's modified UTF-7 (RFC 3501, section 5.1.3), strictly.
///
/// Text between `&` and `-` is UTF-16BE in base64 with `,` in place of `/` and no padding; `&-` is
/// a literal `&`. Anything the RFC forbids is rejected, so decoding stays one-to-one and two
/// server names can never decode to the same folder: `/` inside a shifted section, a section
/// that encodes printable ASCII, a section right after another one, non-zero padding bits.
///
/// A name with non-ASCII characters cannot be modified UTF-7, so it is a server sending UTF-8
/// against the RFC; it is kept as it is rather than lost.
///
/// # Returns
///
/// The decoded name, or `None` when the name is not valid modified UTF-7.
pub(crate) fn decode(name: &str) -> Option<String> {
    if !name.is_ascii() {
        return Some(name.to_owned());
    }
    let mut decoded = String::with_capacity(name.len());
    let mut rest = name;
    let mut after_section = false;
    while let Some(start) = rest.find('&') {
        let (plain, shifted) = rest.split_at(start);
        if after_section && plain.is_empty() {
            // `&...-&...-`: two sections must be one.
            return None;
        }
        decoded.push_str(plain);
        // `shifted` starts with the `&`; the section runs to the next `-`.
        let (section, after) = shifted.get(1..)?.split_once('-')?;
        if section.is_empty() {
            decoded.push('&');
            after_section = false;
        } else {
            decoded.push_str(&decode_section(section)?);
            after_section = true;
        }
        rest = after;
    }
    decoded.push_str(rest);
    Some(decoded)
}

fn decode_section(section: &str) -> Option<String> {
    if section.contains('/') {
        return None;
    }
    let bytes = STANDARD_NO_PAD.decode(section.replace(',', "/")).ok()?;
    // UTF-16 units are pairs of bytes; a leftover byte means the section is broken.
    let (pairs, leftover) = bytes.as_chunks::<2>();
    if !leftover.is_empty() {
        return None;
    }
    let text = char::decode_utf16(pairs.iter().copied().map(u16::from_be_bytes))
        .collect::<Result<String, _>>()
        .ok()?;
    // Printable ASCII must be written as itself, never shifted.
    if text
        .chars()
        .any(|character| (' '..='~').contains(&character))
    {
        return None;
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc_3501_examples_decode() {
        assert_eq!(
            decode("~peter/mail/&U,BTFw-/&ZeVnLIqe-").as_deref(),
            Some("~peter/mail/台北/日本語"),
            "RFC 3501, 5.1.3"
        );
        assert_eq!(decode("&Jjo-!").as_deref(), Some("☺!"), "RFC 2152 smiley");
    }

    #[test]
    fn plain_names_and_escaped_ampersands_pass() {
        assert_eq!(decode("INBOX").as_deref(), Some("INBOX"), "ASCII");
        assert_eq!(decode("Tom &- Jerry").as_deref(), Some("Tom & Jerry"), "&-");
        assert_eq!(
            decode("Entw&APw-rfe").as_deref(),
            Some("Entwürfe"),
            "German drafts"
        );
    }

    #[test]
    fn utf8_names_from_servers_that_ignore_the_rfc_are_kept() {
        assert_eq!(decode("Entwürfe").as_deref(), Some("Entwürfe"), "kept");
    }

    #[test]
    fn malformed_sections_are_rejected() {
        let cases = [
            "&U,BTFw", // no closing `-`
            "&U-",     // too short for one UTF-16 unit
            "&AAAA-",  // fine base64 but 3 bytes, not 2
            "&2D0-",   // an unpaired high surrogate (0xD83D)
            "&!!!-",   // not base64
        ];

        for case in cases {
            assert_eq!(decode(case), None, "rejected: {case}");
        }
    }

    #[test]
    fn encodings_the_rfc_forbids_are_rejected_so_names_never_collide() {
        let cases = [
            "a&AGI-",     // `b` shifted: printable ASCII must be literal
            "&AOQ-&APY-", // two sections in a row (should be `&AOQA9g-`)
            "&U/BTFw-",   // `/` instead of `,`
            "&APx-",      // non-zero padding bits (canonical is `&APw-`)
        ];

        for case in cases {
            assert_eq!(decode(case), None, "rejected: {case}");
        }
        assert_eq!(
            decode("&AOQA9g-").as_deref(),
            Some("äö"),
            "the canonical form"
        );
    }
}
