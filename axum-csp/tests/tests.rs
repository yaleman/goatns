use axum_csp::{CspDirective, CspDirectiveType, CspUrlMatcher, CspValue};
use regex::RegexSet;

#[test]
fn test_example() {
    let csp_matchers = vec![CspUrlMatcher {
        matcher: RegexSet::new([r#"/hello"#]).unwrap(),
        directives: vec![CspDirective::from(
            CspDirectiveType::DefaultSrc,
            vec![CspValue::SelfSite],
        )],
    }];

    assert!(!csp_matchers.is_empty());
    for matcher in csp_matchers {
        assert!(matcher.matcher.is_match("/hello"));
    }
}
