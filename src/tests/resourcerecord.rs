use crate::resourcerecord::check_long_labels;

#[test]
fn test_check_long_labels() {
    assert_eq!(false, check_long_labels(&"hello.".to_string()));
    assert_eq!(false, check_long_labels(&"hello.world".to_string()));
    assert_eq!(
        true,
        check_long_labels(
            &"foo.12345678901234567890123456789012345678901234567890123456789012345678901234567890"
                .to_string()
        )
    );
}
