use encore_core::AccountProfile;
use serde_json::Value;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct AccountMenu {
    pub profile: AccountProfile,
    pub channel_id: Option<String>,
    pub datasync_id: Option<String>,
    pub visitor_data: Option<String>,
    pub logged_out: bool,
    pub has_header: bool,
    pub shape: String,
}

impl AccountMenu {
    pub(crate) fn id(&self) -> Option<&str> {
        self.datasync_id.as_deref().or(self.channel_id.as_deref())
    }
}

pub(crate) fn parse_account_menu(body: &str) -> Result<AccountMenu, serde_json::Error> {
    let root: Value = serde_json::from_str(body)?;
    let context = root.get("responseContext");
    let main = context.and_then(|context| context.get("mainAppWebResponseContext"));
    let header = find_first(&root, "activeAccountHeaderRenderer");

    let channel_id = header.and_then(channel_browse_id);
    let datasync_id = main.and_then(datasync_id);
    let visitor_data = context
        .and_then(|context| context.get("visitorData"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|visitor| !visitor.is_empty())
        .map(str::to_owned);

    Ok(AccountMenu {
        profile: header.map(parse_profile).unwrap_or_default(),
        shape: describe_shape(context, main, header.is_some(), channel_id.is_some()),
        channel_id,
        datasync_id,
        visitor_data,
        logged_out: main
            .and_then(|main| main.get("loggedOut"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        has_header: header.is_some(),
    })
}

pub(crate) fn parse_selected_identity(body: &str) -> Result<Option<String>, serde_json::Error> {
    let root: Value = serde_json::from_str(body)?;
    let response_id = root
        .get("responseContext")
        .and_then(|context| context.get("mainAppWebResponseContext"))
        .and_then(datasync_id);

    let mut identities = Vec::new();
    let sections = find_all(&root, "accountSectionListRenderer");
    let items: Vec<&Value> = match sections.is_empty() {
        true => find_all(&root, "accountItem"),
        false => sections
            .into_iter()
            .flat_map(|section| find_all(section, "accountItem"))
            .collect(),
    };
    for item in items {
        if let Some(identity) = parse_identity_item(item, response_id.as_deref())
            && identities
                .iter()
                .all(|known: &AccountIdentity| known.id != identity.id)
        {
            identities.push(identity);
        }
    }

    let selected = identities.iter().find(|identity| identity.selected);
    let only = (identities.len() == 1).then(|| &identities[0]);
    Ok(selected.or(only).map(|identity| identity.id.clone()))
}

struct AccountIdentity {
    id: String,
    selected: bool,
}

fn parse_identity_item(item: &Value, response_id: Option<&str>) -> Option<AccountIdentity> {
    if item.get("isDisabled").and_then(Value::as_bool) == Some(true)
        || item.get("hasChannel").and_then(Value::as_bool) == Some(false)
    {
        return None;
    }
    let tokens = item
        .get("serviceEndpoint")?
        .get("selectActiveIdentityEndpoint")?
        .get("supportedTokens")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();

    let from_datasync_token = tokens.iter().find_map(|token| {
        let value = token
            .get("datasyncIdToken")
            .or_else(|| token.get("dataSyncIdToken"))?;
        value
            .get("datasyncIdToken")
            .or_else(|| value.get("datasyncId"))
            .or_else(|| value.get("dataSyncId"))
            .and_then(Value::as_str)
            .and_then(account_part)
            .map(str::to_owned)
    });
    let from_page_token = tokens.iter().find_map(|token| {
        token
            .get("pageIdToken")?
            .get("pageId")?
            .as_str()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
    });
    let selected = item
        .get("isSelected")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let from_response = selected.then_some(response_id).flatten().map(str::to_owned);

    Some(AccountIdentity {
        id: from_datasync_token.or(from_page_token).or(from_response)?,
        selected,
    })
}

pub(crate) fn parse_visitor_data(body: &str) -> Option<String> {
    let start = body.find('[')?;
    let value: Value = serde_json::from_str(&body[start..]).ok()?;
    value
        .get(0)?
        .get(2)?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .find(|candidate| candidate.starts_with("Cgt") || candidate.starts_with("Cgs"))
        .map(str::to_owned)
}

fn datasync_id(main: &Value) -> Option<String> {
    main.get("datasyncId")
        .or_else(|| main.get("dataSyncId"))
        .and_then(Value::as_str)
        .and_then(account_part)
        .map(str::to_owned)
}

fn account_part(datasync_id: &str) -> Option<&str> {
    let (first, second) = datasync_id.split_once("||").unwrap_or((datasync_id, ""));
    [first, second]
        .into_iter()
        .map(str::trim)
        .find(|part| !part.is_empty())
}

fn parse_profile(header: &Value) -> AccountProfile {
    let handle = text(header.get("channelHandle"));
    let email = text(header.get("email"));
    AccountProfile {
        name: text(header.get("accountName")).unwrap_or_default(),
        detail: handle.or(email),
        avatar_url: header.get("accountPhoto").and_then(last_thumbnail),
    }
}

fn channel_browse_id(node: &Value) -> Option<String> {
    match node {
        Value::Object(map) => map
            .get("browseId")
            .and_then(Value::as_str)
            .filter(|id| id.starts_with("UC"))
            .map(str::to_owned)
            .or_else(|| map.values().find_map(channel_browse_id)),
        Value::Array(items) => items.iter().find_map(channel_browse_id),
        _ => None,
    }
}

pub(crate) fn find_first<'a>(node: &'a Value, key: &str) -> Option<&'a Value> {
    match node {
        Value::Object(map) => map
            .get(key)
            .or_else(|| map.values().find_map(|child| find_first(child, key))),
        Value::Array(items) => items.iter().find_map(|child| find_first(child, key)),
        _ => None,
    }
}

pub(crate) fn find_all<'a>(root: &'a Value, key: &str) -> Vec<&'a Value> {
    fn walk<'a>(node: &'a Value, key: &str, found: &mut Vec<&'a Value>) {
        match node {
            Value::Object(map) => {
                for (name, child) in map {
                    if name == key {
                        found.push(child);
                    }
                    walk(child, key, found);
                }
            }
            Value::Array(items) => items.iter().for_each(|child| walk(child, key, found)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    walk(root, key, &mut found);
    found
}

pub(crate) fn find_all_shallow<'a>(root: &'a Value, key: &str) -> Vec<&'a Value> {
    fn walk<'a>(node: &'a Value, key: &str, found: &mut Vec<&'a Value>) {
        match node {
            Value::Object(map) => {
                for (name, child) in map {
                    match name == key {
                        true => found.push(child),
                        false => walk(child, key, found),
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|child| walk(child, key, found)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    walk(root, key, &mut found);
    found
}

pub(crate) fn text(node: Option<&Value>) -> Option<String> {
    let node = node?;
    let runs = node.get("runs").and_then(Value::as_array).map(|runs| {
        runs.iter()
            .filter_map(|run| run.get("text").and_then(Value::as_str))
            .collect::<String>()
    });
    let value = runs
        .or_else(|| {
            node.get("simpleText")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .or_else(|| node.as_str().map(str::to_owned))?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

pub(crate) fn artist_id(node: Option<&Value>) -> Option<String> {
    node?
        .get("runs")?
        .as_array()?
        .iter()
        .filter_map(|run| {
            run.get("navigationEndpoint")?
                .get("browseEndpoint")?
                .get("browseId")?
                .as_str()
        })
        .find(|id| id.starts_with("UC"))
        .map(str::to_owned)
}

pub(crate) fn text_with_links(node: Option<&Value>) -> Option<String> {
    let node = node?;
    let Some(runs) = node.get("runs").and_then(Value::as_array) else {
        return text(Some(node));
    };
    let value = runs
        .iter()
        .filter_map(|run| {
            let shown = run.get("text").and_then(Value::as_str)?;
            if !shown.starts_with("http") {
                return Some(shown);
            }
            let endpoint = run.get("navigationEndpoint");
            let url = endpoint
                .and_then(|endpoint| endpoint.get("urlEndpoint"))
                .and_then(|endpoint| endpoint.get("url"))
                .and_then(Value::as_str)
                .filter(|url| url.starts_with("http"));
            Some(url.unwrap_or(shown))
        })
        .collect::<String>();
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn last_thumbnail(photo: &Value) -> Option<String> {
    let url = photo
        .get("thumbnails")
        .and_then(Value::as_array)
        .and_then(|thumbnails| thumbnails.last())
        .and_then(|thumbnail| thumbnail.get("url"))
        .and_then(Value::as_str)?
        .trim();
    match url {
        "" => None,
        url if url.starts_with("//") => Some(format!("https:{url}")),
        url => Some(url.to_owned()),
    }
}

fn describe_shape(
    context: Option<&Value>,
    main: Option<&Value>,
    has_header: bool,
    has_channel_id: bool,
) -> String {
    let names = |node: Option<&Value>| match node.and_then(Value::as_object) {
        Some(fields) => fields.keys().cloned().collect::<Vec<_>>().join(", "),
        None => "absent".to_owned(),
    };
    format!(
        "responseContext: [{}]; mainAppWebResponseContext: [{}]; account header: {}; channel id: {}",
        names(context),
        names(main),
        if has_header { "present" } else { "absent" },
        if has_channel_id { "present" } else { "absent" },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu(body: &str) -> AccountMenu {
        parse_account_menu(body).unwrap()
    }

    const HEADER: &str = r#""activeAccountHeaderRenderer": {
        "accountName": {"runs": [{"text": "Personal channel"}]},
        "channelHandle": {"runs": [{"text": "@personal"}]},
        "email": {"runs": [{"text": "listener@example.invalid"}]},
        "accountPhoto": {"thumbnails": [
            {"url": "https://img.example/small.jpg"},
            {"url": "https://img.example/large.jpg"}
        ]},
        "navigationEndpoint": {"browseEndpoint": {"browseId": "UCSANITIZEDPERSONAL"}}
    }"#;

    fn with_header(context: &str) -> String {
        format!(
            r#"{{
                "responseContext": {context},
                "actions": [{{"openPopupAction": {{"popup": {{"multiPageMenuRenderer": {{
                    "sections": [{{{HEADER}}}]
                }}}}}}}}]
            }}"#
        )
    }

    #[test]
    fn the_profile_comes_from_the_active_account_header() {
        let menu = menu(&with_header(r#"{"visitorData": "CgtLOGIN"}"#));

        assert_eq!(
            menu.profile,
            AccountProfile {
                name: "Personal channel".into(),
                detail: Some("@personal".into()),
                avatar_url: Some("https://img.example/large.jpg".into()),
            }
        );
        assert!(menu.has_header);
    }

    #[test]
    fn the_login_bound_visitor_data_is_read_from_the_response_context() {
        let menu = menu(&with_header(r#"{"visitorData": "CgtLOGIN"}"#));

        assert_eq!(menu.visitor_data.as_deref(), Some("CgtLOGIN"));
    }

    #[test]
    fn the_channel_id_comes_from_the_header_and_must_look_like_a_channel() {
        let with_channel = menu(&with_header("{}"));
        let without = menu(&with_header("{}").replace("UCSANITIZEDPERSONAL", "not-a-channel"));

        assert_eq!(
            with_channel.channel_id.as_deref(),
            Some("UCSANITIZEDPERSONAL")
        );
        assert_eq!(without.channel_id, None);
    }

    #[test]
    fn a_primary_channel_datasync_id_is_the_part_before_the_separator() {
        let menu = menu(&with_header(
            r#"{"mainAppWebResponseContext": {"datasyncId": "108123||"}}"#,
        ));

        assert_eq!(menu.datasync_id.as_deref(), Some("108123"));
    }

    #[test]
    fn a_secondary_channel_datasync_id_is_its_own_part_not_the_users() {
        let menu = menu(&with_header(
            r#"{"mainAppWebResponseContext": {"datasyncId": "channel-sync||user-sync"}}"#,
        ));

        assert_eq!(menu.datasync_id.as_deref(), Some("channel-sync"));
    }

    #[test]
    fn the_part_after_the_separator_is_used_when_the_first_is_empty() {
        let menu = menu(&with_header(
            r#"{"mainAppWebResponseContext": {"datasyncId": "||user-sync"}}"#,
        ));

        assert_eq!(menu.datasync_id.as_deref(), Some("user-sync"));
    }

    #[test]
    fn blank_datasync_ids_are_missing() {
        for blank in [r#""""#, r#""||""#, "7"] {
            let context = format!(r#"{{"mainAppWebResponseContext": {{"datasyncId": {blank}}}}}"#);

            assert_eq!(menu(&with_header(&context)).datasync_id, None, "{blank}");
        }
    }

    #[test]
    fn the_datasync_id_wins_over_the_channel_id() {
        let menu = menu(&with_header(
            r#"{"mainAppWebResponseContext": {"datasyncId": "108123||"}}"#,
        ));

        assert_eq!(menu.id(), Some("108123"));
    }

    #[test]
    fn the_channel_id_is_the_id_when_there_is_no_datasync_id() {
        assert_eq!(menu(&with_header("{}")).id(), Some("UCSANITIZEDPERSONAL"));
    }

    #[test]
    fn a_logged_out_flag_is_read() {
        let menu =
            menu(r#"{"responseContext": {"mainAppWebResponseContext": {"loggedOut": true}}}"#);

        assert!(menu.logged_out);
    }

    #[test]
    fn the_email_is_the_detail_when_there_is_no_handle() {
        let body =
            with_header("{}").replace(r#""channelHandle": {"runs": [{"text": "@personal"}]},"#, "");

        assert_eq!(
            menu(&body).profile.detail.as_deref(),
            Some("listener@example.invalid")
        );
    }

    #[test]
    fn text_may_be_split_into_runs_or_plain() {
        let body = with_header("{}")
            .replace(
                r#"{"runs": [{"text": "Personal channel"}]}"#,
                r#"{"runs": [{"text": "Personal "}, {"text": "channel"}]}"#,
            )
            .replace(
                r#""channelHandle": {"runs": [{"text": "@personal"}]}"#,
                r#""channelHandle": {"simpleText": "@personal"}"#,
            );

        let profile = menu(&body).profile;

        assert_eq!(profile.name, "Personal channel");
        assert_eq!(profile.detail.as_deref(), Some("@personal"));
    }

    #[test]
    fn a_protocol_relative_avatar_url_becomes_https() {
        let body =
            with_header("{}").replace("https://img.example/large.jpg", "//img.example/large.jpg");

        assert_eq!(
            menu(&body).profile.avatar_url.as_deref(),
            Some("https://img.example/large.jpg")
        );
    }

    #[test]
    fn blank_header_fields_are_treated_as_missing() {
        let body = with_header("{}")
            .replace(r#"{"text": "Personal channel"}"#, r#"{"text": "  "}"#)
            .replace(r#""url": "https://img.example/large.jpg""#, r#""url": " ""#);

        let profile = menu(&body).profile;

        assert_eq!(profile.name, "");
        assert_eq!(profile.avatar_url, None);
    }

    #[test]
    fn a_response_with_nothing_useful_has_no_id_and_no_header() {
        let menu = menu(r#"{"responseContext": {"serviceTrackingParams": []}}"#);

        assert_eq!(menu.id(), None);
        assert!(!menu.has_header);
        assert_eq!(menu.profile, AccountProfile::default());
    }

    #[test]
    fn the_shape_lists_field_names_and_never_values() {
        let menu = menu(&with_header(
            r#"{"visitorData": "super-secret-visitor", "mainAppWebResponseContext": {"loggedOut": false}}"#,
        ));

        assert!(menu.shape.contains("visitorData"), "{}", menu.shape);
        assert!(menu.shape.contains("loggedOut"), "{}", menu.shape);
        assert!(
            menu.shape.contains("account header: present"),
            "{}",
            menu.shape
        );
        assert!(
            !menu.shape.contains("super-secret-visitor"),
            "{}",
            menu.shape
        );
        assert!(
            !menu.shape.contains("UCSANITIZEDPERSONAL"),
            "{}",
            menu.shape
        );
        assert!(
            !menu.shape.contains("listener@example.invalid"),
            "{}",
            menu.shape
        );
    }

    #[test]
    fn something_that_is_not_json_is_an_error() {
        assert!(parse_account_menu("<html>").is_err());
        assert!(parse_selected_identity("<html>").is_err());
    }

    fn accounts_list(items: &str, response_context: &str) -> String {
        format!(
            r#"{{
                "responseContext": {response_context},
                "actions": [{{"getMultiPageMenuAction": {{"menu": {{"multiPageMenuRenderer": {{
                    "sections": [{{"accountSectionListRenderer": {{
                        "contents": [{{"accountItemSectionRenderer": {{"contents": [{items}]}}}}]
                    }}}}]
                }}}}}}}}]
            }}"#
        )
    }

    fn item(name: &str, selected: bool, token: &str) -> String {
        format!(
            r#"{{"accountItem": {{
                "accountName": {{"runs": [{{"text": "{name}"}}]}},
                "isSelected": {selected},
                "hasChannel": true,
                "serviceEndpoint": {{"selectActiveIdentityEndpoint": {{"supportedTokens": [{token}]}}}}
            }}}}"#
        )
    }

    fn datasync_token(id: &str) -> String {
        format!(r#"{{"datasyncIdToken": {{"datasyncIdToken": "{id}"}}}}"#)
    }

    #[test]
    fn a_single_listed_identity_is_the_account() {
        let body = accounts_list(
            &item("Default", true, &datasync_token("default-sync||device")),
            "{}",
        );

        assert_eq!(
            parse_selected_identity(&body).unwrap().as_deref(),
            Some("default-sync")
        );
    }

    #[test]
    fn with_several_identities_the_selected_one_is_the_account() {
        let items = format!(
            "{},{}",
            item("Default", false, &datasync_token("default-sync")),
            item(
                "Brand",
                true,
                r#"{"pageIdToken": {"pageId": "brand-page-id"}}"#
            ),
        );

        let id = parse_selected_identity(&accounts_list(&items, "{}")).unwrap();

        assert_eq!(id.as_deref(), Some("brand-page-id"));
    }

    #[test]
    fn several_identities_with_none_selected_choose_nothing() {
        let items = format!(
            "{},{}",
            item("A", false, &datasync_token("a-sync")),
            item("B", false, &datasync_token("b-sync")),
        );

        assert_eq!(
            parse_selected_identity(&accounts_list(&items, "{}")).unwrap(),
            None
        );
    }

    #[test]
    fn a_selected_item_without_a_token_falls_back_to_the_response_context_id() {
        let body = accounts_list(
            &item("Default", true, ""),
            r#"{"mainAppWebResponseContext": {"datasyncId": "ctx-sync||"}}"#,
        );

        assert_eq!(
            parse_selected_identity(&body).unwrap().as_deref(),
            Some("ctx-sync")
        );
    }

    #[test]
    fn disabled_and_channelless_items_are_skipped() {
        let disabled = item("A", true, &datasync_token("a-sync"))
            .replace(r#""hasChannel": true"#, r#""hasChannel": false"#);

        assert_eq!(
            parse_selected_identity(&accounts_list(&disabled, "{}")).unwrap(),
            None
        );
    }

    #[test]
    fn a_list_with_no_items_chooses_nothing() {
        assert_eq!(
            parse_selected_identity(r#"{"responseContext": {}}"#).unwrap(),
            None
        );
    }

    #[test]
    fn the_visitor_data_is_found_after_the_junk_prefix() {
        let body = r#")]}'
[[1,2,["noise","CgtREAL_VISITOR","other"]]]"#;

        assert_eq!(parse_visitor_data(body).as_deref(), Some("CgtREAL_VISITOR"));
    }

    #[test]
    fn visitor_data_that_cannot_be_found_is_none() {
        assert_eq!(parse_visitor_data("not json at all"), None);
        assert_eq!(parse_visitor_data(")]}'\n[[1,2,[\"nothing\"]]]"), None);
        assert_eq!(parse_visitor_data(")]}'\n[]"), None);
    }

    #[test]
    fn the_artist_link_in_a_byline_gives_the_artist_id() {
        let byline: Value = serde_json::from_str(
            r#"{"runs": [
                {"text": "Kenshi Yonezu", "navigationEndpoint": {"browseEndpoint": {"browseId": "UCabc"}}},
                {"text": " • "},
                {"text": "Lemon", "navigationEndpoint": {"browseEndpoint": {"browseId": "MPREb_x"}}}
            ]}"#,
        )
        .unwrap();

        assert_eq!(artist_id(Some(&byline)).as_deref(), Some("UCabc"));
    }

    #[test]
    fn a_byline_without_an_artist_link_gives_no_id() {
        let byline: Value =
            serde_json::from_str(r#"{"runs": [{"text": "Various Artists"}]}"#).unwrap();

        assert_eq!(artist_id(Some(&byline)), None);
    }
}
