use std::collections::BTreeMap;

use wry::cookie::Cookie;

pub fn youtube_cookie_header(mut cookies: Vec<Cookie<'static>>) -> String {
    cookies.sort_by_key(|cookie| domain_of(cookie).len());
    let mut jar = BTreeMap::new();
    for cookie in cookies {
        if is_youtube_domain(domain_of(&cookie)) {
            jar.insert(cookie.name().to_owned(), cookie.value().to_owned());
        }
    }
    jar.into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; ")
}

fn domain_of<'a>(cookie: &'a Cookie<'_>) -> &'a str {
    cookie.domain().unwrap_or_default().trim_start_matches('.')
}

fn is_youtube_domain(domain: &str) -> bool {
    domain == "youtube.com" || domain.ends_with(".youtube.com")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookie(raw: &str) -> Cookie<'static> {
        Cookie::parse(raw.to_owned()).unwrap()
    }

    #[test]
    fn keeps_youtube_cookies_and_drops_everything_else() {
        let header = youtube_cookie_header(vec![
            cookie("SAPISID=abc; Domain=.youtube.com"),
            cookie("SID=def; Domain=.youtube.com"),
            cookie("VISITOR_INFO1_LIVE=xyz; Domain=music.youtube.com"),
            cookie("SAPISID=notthisone; Domain=.google.com"),
            cookie("nodomain=1"),
            cookie("lookalike=1; Domain=notyoutube.com"),
        ]);

        assert_eq!(header, "SAPISID=abc; SID=def; VISITOR_INFO1_LIVE=xyz");
    }

    #[test]
    fn the_most_specific_domain_wins_a_name_collision() {
        let header = youtube_cookie_header(vec![
            cookie("PREF=specific; Domain=music.youtube.com"),
            cookie("PREF=broad; Domain=.youtube.com"),
        ]);

        assert_eq!(header, "PREF=specific");
    }
}
