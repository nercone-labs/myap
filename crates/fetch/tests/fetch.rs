use fetch::{Method, Query, Request, Response};

#[test]
fn query_encodes_reserved_characters() {
    // RFC 3986 §2.3: only ALPHA, DIGIT, "-", ".", "_", "~" stay unencoded.
    let q = Query::new().pair("q", "東京 駅&x=1").pair("a-b_c.d~", "/?#[]@!$'()*+,;=%");
    assert_eq!(q.as_str(), "q=%E6%9D%B1%E4%BA%AC%20%E9%A7%85%26x%3D1&a-b_c.d~=%2F%3F%23%5B%5D%40%21%24%27%28%29%2A%2B%2C%3B%3D%25");
    assert_eq!(Query::new().optional("x", None::<&str>).as_str(), "");
    assert_eq!(Query::new().pair("a", "1").append_to("https://x/y"), "https://x/y?a=1");
    assert_eq!(Query::new().pair("a", "1").append_to("https://x/y?b=2"), "https://x/y?b=2&a=1");
}

#[test]
fn decode_round_trip() {
    let text = "東京 駅&x=1/%";
    assert_eq!(Query::decode(&Query::encode(text)), text);
    assert_eq!(Query::decode("%zz%4"), "%zz%4");
    assert_eq!(Query::decode("%41"), "A");
}

#[test]
fn request_host() {
    assert_eq!(Request::get("https://User@Example.org:8080/a?b").host().as_deref(), Some("example.org"));
    assert_eq!(Request::get("http://[::1]:80/").host().as_deref(), Some("::1"));
    assert_eq!(Request::get("not a url").host(), None);
    let r = Request::post("https://x/", "application/json", "{}").header("User-Agent", "t");
    assert_eq!(r.method, Method::Post);
    assert_eq!(r.header_value("content-type"), Some("application/json"));
}

#[test]
fn response_success() {
    assert!(Response::ok("x").is_success());
    assert!(!Response::failed("offline").is_success());
    assert!(!Response { status: 404, ..Default::default() }.is_success());
}
