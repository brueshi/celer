/// Parse a query string into key-value pairs.
///
/// Handles URL-encoded format: `key1=value1&key2=value2`
/// Does NOT perform URL decoding (values are returned raw).
pub fn parse_query(query: &str) -> Vec<(String, String)> {
    if query.is_empty() {
        return Vec::new();
    }

    query
        .split('&')
        .filter(|s| !s.is_empty())
        .map(|pair| {
            match pair.split_once('=') {
                Some((k, v)) => (k.to_string(), v.to_string()),
                None => (pair.to_string(), String::new()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query() {
        assert!(parse_query("").is_empty());
    }

    #[test]
    fn single_param() {
        let params = parse_query("key=value");
        assert_eq!(params, vec![("key".into(), "value".into())]);
    }

    #[test]
    fn multiple_params() {
        let params = parse_query("a=1&b=2&c=3");
        assert_eq!(params.len(), 3);
        assert_eq!(params[0], ("a".into(), "1".into()));
        assert_eq!(params[1], ("b".into(), "2".into()));
        assert_eq!(params[2], ("c".into(), "3".into()));
    }

    #[test]
    fn key_without_value() {
        let params = parse_query("flag");
        assert_eq!(params, vec![("flag".into(), String::new())]);
    }

    #[test]
    fn empty_value() {
        let params = parse_query("key=");
        assert_eq!(params, vec![("key".into(), String::new())]);
    }

    #[test]
    fn trailing_ampersand() {
        let params = parse_query("a=1&");
        assert_eq!(params.len(), 1);
    }
}
