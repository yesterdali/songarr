//! Subsonic auth plumbing.
//!
//! User credentials are NEVER validated here — they pass through to
//! Navidrome verbatim (protocol rule #1). This module only extracts request
//! params for routing decisions and builds auth for proxy-originated calls
//! made with the configured admin account.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::config::Navidrome;

/// Decode query params (last occurrence wins, like Navidrome's parser).
pub fn query_params(query: &str) -> HashMap<Cow<'_, str>, Cow<'_, str>> {
    url::form_urlencoded::parse(query.as_bytes()).collect()
}

/// Auth query for proxy-originated requests: `t = md5(password + salt)`.
pub fn admin_auth_query(navidrome: &Navidrome) -> String {
    let salt = uuid::Uuid::new_v4().simple().to_string();
    let token = format!(
        "{:x}",
        md5::compute(format!("{}{}", navidrome.admin_password, salt))
    );
    format!(
        "u={}&t={token}&s={salt}&v=1.16.1&c=songarr",
        urlencode(&navidrome.admin_user)
    )
}

fn urlencode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_decode_percent_and_plus() {
        let params = query_params("u=alice&query=a%20b+c&f=json");
        assert_eq!(params.get("u").map(|v| v.as_ref()), Some("alice"));
        assert_eq!(params.get("query").map(|v| v.as_ref()), Some("a b c"));
        assert_eq!(params.get("missing"), None);
    }

    #[test]
    fn admin_auth_token_is_md5_of_password_plus_salt() {
        let navidrome = Navidrome {
            base_url: "http://x".into(),
            admin_user: "admin".into(),
            admin_password: "sesame".into(),
        };
        let query = admin_auth_query(&navidrome);
        let params = query_params(&query);
        let token = params.get("t").unwrap().to_string();
        let salt = params.get("s").unwrap().to_string();
        assert_eq!(
            token,
            format!("{:x}", md5::compute(format!("sesame{salt}")))
        );
    }
}
