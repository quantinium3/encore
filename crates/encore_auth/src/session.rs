const AUTH_COOKIES: [&str; 2] = ["SAPISID", "__Secure-3PAPISID"];

fn sapisid(cookie_header: &str) -> Option<&str> {
    AUTH_COOKIES.iter().find_map(|wanted| {
        cookie_header.split(';').find_map(|pair| {
            let (name, value) = pair.split_once('=')?;
            let value = value.trim();
            (name.trim() == *wanted && !value.is_empty()).then_some(value)
        })
    })
}

pub fn has_session(cookie_header: &str) -> bool {
    sapisid(cookie_header).is_some()
}

fn sorted_pairs(header: &str) -> Vec<&str> {
    let mut pairs: Vec<&str> = header.split(';').map(str::trim).collect();
    pairs.sort_unstable();
    pairs
}

pub fn same_jar(a: &str, b: &str) -> bool {
    sorted_pairs(a) == sorted_pairs(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_session_needs_an_auth_cookie() {
        assert!(has_session("SID=1; SAPISID=abc"));
        assert!(has_session("__Secure-3PAPISID=abc"));
        assert!(!has_session("VISITOR_INFO1_LIVE=xyz; SID=1"));
        assert!(!has_session(""));
    }

    #[test]
    fn an_empty_auth_cookie_is_not_a_session() {
        assert!(!has_session("SAPISID=; SID=1"));
        assert!(!has_session("SAPISID=  "));
        assert!(has_session("SAPISID=; __Secure-3PAPISID=abc"));
    }

    #[test]
    fn a_reordered_jar_is_the_same_jar() {
        assert!(same_jar("SAPISID=a; SID=b", "SID=b; SAPISID=a"));
    }

    #[test]
    fn a_rotated_value_is_a_different_jar() {
        assert!(!same_jar(
            "SAPISID=a; __Secure-3PSIDTS=new",
            "SAPISID=a; __Secure-3PSIDTS=old"
        ));
    }
}
