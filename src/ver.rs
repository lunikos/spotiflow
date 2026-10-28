use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Ver(pub [u32; 4]);

impl Ver {
    pub fn parse(s: &str) -> Option<Ver> {
        let mut v = [0u32; 4];
        let parts: Vec<&str> = s
            .trim()
            .split('.')
            .take_while(|p| !p.starts_with('g'))
            .collect();
        if parts.is_empty() || parts.len() > 4 {
            return None;
        }
        for (i, p) in parts.iter().enumerate() {
            v[i] = p.parse().ok()?;
        }
        Some(Ver(v))
    }
}

impl fmt::Display for Ver {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let [a, b, c, d] = self.0;
        write!(f, "{a}.{b}.{c}.{d}")
    }
}

#[cfg(test)]
mod tests {
    use super::Ver;

    #[test]
    fn parses_and_orders() {
        assert_eq!(
            Ver::parse("1.3.6.1921.g7086c071a617"),
            Some(Ver([1, 3, 6, 1921]))
        );
        assert_eq!(Ver::parse("1.2"), Some(Ver([1, 2, 0, 0])));
        assert!(Ver::parse("1.2.99.9").unwrap() < Ver::parse("1.3.0.0").unwrap());
        assert_eq!(Ver::parse("x.1"), None);
    }
}
