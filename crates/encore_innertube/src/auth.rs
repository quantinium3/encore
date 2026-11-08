use std::time::{SystemTime, UNIX_EPOCH};

use sha1::{Digest as _, Sha1};

pub(crate) fn sapisid_hash(sapisid: &str, origin: &str) -> String {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    sapisid_hash_at(epoch, sapisid, origin)
}

fn sapisid_hash_at(epoch: u64, sapisid: &str, origin: &str) -> String {
    let digest = Sha1::digest(format!("{epoch} {sapisid} {origin}").as_bytes());
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("SAPISIDHASH {epoch}_{hex}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_sha1_of_the_epoch_the_sapisid_and_the_origin() {
        assert_eq!(
            sapisid_hash_at(1_700_000_000, "abc", "https://music.youtube.com"),
            "SAPISIDHASH 1700000000_2f3ec011e870f3fbd0238c090c2062c208cead32"
        );
    }

    #[test]
    fn another_sapisid_or_origin_changes_the_hash() {
        let base = sapisid_hash_at(1, "abc", "https://music.youtube.com");

        assert_ne!(base, sapisid_hash_at(1, "abd", "https://music.youtube.com"));
        assert_ne!(base, sapisid_hash_at(1, "abc", "https://www.youtube.com"));
        assert_ne!(base, sapisid_hash_at(2, "abc", "https://music.youtube.com"));
    }

    #[test]
    fn the_live_hash_has_the_header_shape_and_never_contains_the_sapisid() {
        let header = sapisid_hash("supersecretvalue", "https://music.youtube.com");

        let rest = header.strip_prefix("SAPISIDHASH ").unwrap();
        let (epoch, digest) = rest.split_once('_').unwrap();
        assert!(epoch.parse::<u64>().unwrap() > 0);
        assert_eq!(digest.len(), 40);
        assert!(digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(!header.contains("supersecretvalue"));
    }
}
