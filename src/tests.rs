#[cfg(test)]
mod tests {
    use crate::utils::name_as_bytes;
    #[test]
    fn test_resourcerecord_name_to_bytes() {
        let rdata = String::from("cheese.world");
        assert_eq!(
            name_as_bytes(rdata),
            [6, 99, 104, 101, 101, 115, 101, 5, 119, 111, 114, 108, 100, 0]
        );
    }
    #[test]
    fn test_resourcerecord_short_name_to_bytes() {
        let rdata = String::from("cheese");
        assert_eq!(name_as_bytes(rdata), [6, 99, 104, 101, 101, 115, 101, 0]);
    }
}
