use super::generated::*;

#[test]
fn action_applies_base_url_and_api_keys() {
    let request = Api::new()
        .account_key("secret")
        .api_key("query secret")
        .untagged()
        .get_user("42")
        .request()
        .expect("action request");

    assert_eq!(
        request.uri().to_string(),
        "https://api.example.test/v1/users/42?api_key=query%20secret"
    );
    let account_key = http::header::HeaderName::from_bytes(b"AccountKey").unwrap();
    assert_eq!(request.headers().get(account_key).unwrap(), "secret");
}
