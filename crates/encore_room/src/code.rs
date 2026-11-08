use rand::Rng as _;

const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
pub const CODE_LEN: usize = 6;

pub fn generate_code() -> String {
    let mut rng = rand::rng();
    (0..CODE_LEN)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect()
}

pub fn normalize_code(input: &str) -> Option<String> {
    let code: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .flat_map(char::to_uppercase)
        .collect();
    let valid = code.len() == CODE_LEN && code.bytes().all(|b| ALPHABET.contains(&b));
    valid.then_some(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_codes_are_valid() {
        for _ in 0..200 {
            let code = generate_code();
            assert_eq!(normalize_code(&code), Some(code));
        }
    }

    #[test]
    fn normalizes_case_and_separators() {
        assert_eq!(normalize_code(" abc-234 "), Some("ABC234".to_owned()));
    }

    #[test]
    fn rejects_ambiguous_or_short_codes() {
        assert_eq!(normalize_code("ABC23"), None);
        assert_eq!(normalize_code("ABC23O"), None);
        assert_eq!(normalize_code("ABC231"), None);
    }
}
