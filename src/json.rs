// Hand-rolled JSON parser, serializer, and accessors.
// No external dependencies — only std.

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

// ---------------------------------------------------------------------------
// Accessor helpers
// ---------------------------------------------------------------------------

impl JsonValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            JsonValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            JsonValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<JsonValue>> {
        match self {
            JsonValue::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&Vec<(String, JsonValue)>> {
        match self {
            JsonValue::Object(o) => Some(o),
            _ => None,
        }
    }

    /// Look up a key in an object (first match wins).
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        match self {
            JsonValue::Object(pairs) => {
                pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v)
            }
            _ => None,
        }
    }

    /// Nested lookup using a slice of keys.
    pub fn get_path(&self, keys: &[&str]) -> Option<&JsonValue> {
        let mut current = self;
        for key in keys {
            current = current.get(key)?;
        }
        Some(current)
    }
}

// ---------------------------------------------------------------------------
// Serializer
// ---------------------------------------------------------------------------

impl JsonValue {
    pub fn to_json_string(&self) -> String {
        match self {
            JsonValue::Null => "null".to_string(),
            JsonValue::Bool(true) => "true".to_string(),
            JsonValue::Bool(false) => "false".to_string(),
            JsonValue::Number(n) => {
                if n.is_finite() && *n == (*n as i64) as f64 {
                    format!("{}", *n as i64)
                } else {
                    // Use Rust's default float formatting, which is valid JSON.
                    format!("{}", n)
                }
            }
            JsonValue::Str(s) => {
                let mut out = String::with_capacity(s.len() + 2);
                out.push('"');
                for ch in s.chars() {
                    match ch {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        '\r' => out.push_str("\\r"),
                        '\t' => out.push_str("\\t"),
                        '\x08' => out.push_str("\\b"),
                        '\x0C' => out.push_str("\\f"),
                        c if (c as u32) < 0x20 => {
                            out.push_str(&format!("\\u{:04X}", c as u32));
                        }
                        c => out.push(c),
                    }
                }
                out.push('"');
                out
            }
            JsonValue::Array(items) => {
                let mut out = String::from("[");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&item.to_json_string());
                }
                out.push(']');
                out
            }
            JsonValue::Object(pairs) => {
                let mut out = String::from("{");
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    // Serialize the key using the string serializer.
                    out.push_str(&JsonValue::Str(k.clone()).to_json_string());
                    out.push(':');
                    out.push_str(&v.to_json_string());
                }
                out.push('}');
                out
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

struct Parser {
    input: Vec<char>,
    pos: usize,
}

impl Parser {
    fn new(input: &str) -> Self {
        Parser {
            input: input.chars().collect(),
            pos: 0,
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.input.get(self.pos).copied();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    fn expect(&mut self, ch: char) -> Result<(), String> {
        match self.advance() {
            Some(c) if c == ch => Ok(()),
            Some(c) => Err(format!("expected '{}' but got '{}' at pos {}", ch, c, self.pos - 1)),
            None => Err(format!("expected '{}' but reached end of input", ch)),
        }
    }

    fn parse_value(&mut self) -> Result<JsonValue, String> {
        self.skip_ws();
        match self.peek() {
            Some('"') => self.parse_string().map(JsonValue::Str),
            Some('t') | Some('f') => self.parse_bool(),
            Some('n') => self.parse_null(),
            Some('[') => self.parse_array(),
            Some('{') => self.parse_object(),
            Some(c) if c == '-' || c.is_ascii_digit() => self.parse_number(),
            Some(c) => Err(format!("unexpected character '{}' at pos {}", c, self.pos)),
            None => Err("unexpected end of input".to_string()),
        }
    }

    fn parse_null(&mut self) -> Result<JsonValue, String> {
        self.consume_literal("null")?;
        Ok(JsonValue::Null)
    }

    fn parse_bool(&mut self) -> Result<JsonValue, String> {
        if self.peek() == Some('t') {
            self.consume_literal("true")?;
            Ok(JsonValue::Bool(true))
        } else {
            self.consume_literal("false")?;
            Ok(JsonValue::Bool(false))
        }
    }

    fn consume_literal(&mut self, lit: &str) -> Result<(), String> {
        for ch in lit.chars() {
            match self.advance() {
                Some(c) if c == ch => {}
                Some(c) => {
                    return Err(format!(
                        "expected '{}' while parsing literal '{}', got '{}' at pos {}",
                        ch, lit, c, self.pos - 1
                    ))
                }
                None => {
                    return Err(format!(
                        "unexpected end of input while parsing literal '{}'",
                        lit
                    ))
                }
            }
        }
        Ok(())
    }

    fn parse_number(&mut self) -> Result<JsonValue, String> {
        let start = self.pos;
        // Optional minus
        if self.peek() == Some('-') {
            self.pos += 1;
        }
        // Integer part
        if self.peek() == Some('0') {
            self.pos += 1;
        } else if matches!(self.peek(), Some('1'..='9')) {
            while matches!(self.peek(), Some('0'..='9')) {
                self.pos += 1;
            }
        } else {
            return Err(format!("invalid number at pos {}", self.pos));
        }
        // Fractional part
        if self.peek() == Some('.') {
            self.pos += 1;
            if !matches!(self.peek(), Some('0'..='9')) {
                return Err(format!("expected digit after '.' at pos {}", self.pos));
            }
            while matches!(self.peek(), Some('0'..='9')) {
                self.pos += 1;
            }
        }
        // Exponent part
        if matches!(self.peek(), Some('e') | Some('E')) {
            self.pos += 1;
            if matches!(self.peek(), Some('+') | Some('-')) {
                self.pos += 1;
            }
            if !matches!(self.peek(), Some('0'..='9')) {
                return Err(format!("expected digit in exponent at pos {}", self.pos));
            }
            while matches!(self.peek(), Some('0'..='9')) {
                self.pos += 1;
            }
        }
        let raw: String = self.input[start..self.pos].iter().collect();
        raw.parse::<f64>()
            .map(JsonValue::Number)
            .map_err(|e| format!("invalid number '{}': {}", raw, e))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect('"')?;
        let mut s = String::new();
        loop {
            match self.advance() {
                None => return Err("unterminated string".to_string()),
                Some('"') => break,
                Some('\\') => {
                    match self.advance() {
                        Some('"') => s.push('"'),
                        Some('\\') => s.push('\\'),
                        Some('/') => s.push('/'),
                        Some('n') => s.push('\n'),
                        Some('t') => s.push('\t'),
                        Some('r') => s.push('\r'),
                        Some('b') => s.push('\x08'),
                        Some('f') => s.push('\x0C'),
                        Some('u') => {
                            // Read exactly 4 hex digits.
                            let mut hex = String::with_capacity(4);
                            for _ in 0..4 {
                                match self.advance() {
                                    Some(h) if h.is_ascii_hexdigit() => hex.push(h),
                                    Some(c) => {
                                        return Err(format!(
                                            "invalid hex digit '{}' in \\uXXXX escape",
                                            c
                                        ))
                                    }
                                    None => {
                                        return Err(
                                            "unexpected end of input in \\uXXXX escape".to_string()
                                        )
                                    }
                                }
                            }
                            let code = u32::from_str_radix(&hex, 16)
                                .map_err(|e| format!("invalid unicode escape \\u{}: {}", hex, e))?;
                            let ch = char::from_u32(code).ok_or_else(|| {
                                format!("invalid unicode code point U+{:04X}", code)
                            })?;
                            s.push(ch);
                        }
                        Some(c) => return Err(format!("invalid escape sequence '\\{}'", c)),
                        None => return Err("unexpected end of input after '\\'".to_string()),
                    }
                }
                Some(c) => s.push(c),
            }
        }
        Ok(s)
    }

    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.expect('[')?;
        self.skip_ws();
        let mut items = Vec::new();
        if self.peek() == Some(']') {
            self.pos += 1;
            return Ok(JsonValue::Array(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.skip_ws();
            match self.peek() {
                Some(',') => {
                    self.pos += 1;
                }
                Some(']') => {
                    self.pos += 1;
                    break;
                }
                Some(c) => return Err(format!("expected ',' or ']' in array, got '{}'", c)),
                None => return Err("unexpected end of input in array".to_string()),
            }
        }
        Ok(JsonValue::Array(items))
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.expect('{')?;
        self.skip_ws();
        let mut pairs = Vec::new();
        if self.peek() == Some('}') {
            self.pos += 1;
            return Ok(JsonValue::Object(pairs));
        }
        loop {
            self.skip_ws();
            // Key must be a string.
            if self.peek() != Some('"') {
                return Err(format!(
                    "expected string key in object, got {:?} at pos {}",
                    self.peek(),
                    self.pos
                ));
            }
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(':')?;
            let value = self.parse_value()?;
            pairs.push((key, value));
            self.skip_ws();
            match self.peek() {
                Some(',') => {
                    self.pos += 1;
                }
                Some('}') => {
                    self.pos += 1;
                    break;
                }
                Some(c) => return Err(format!("expected ',' or '}}' in object, got '{}'", c)),
                None => return Err("unexpected end of input in object".to_string()),
            }
        }
        Ok(JsonValue::Object(pairs))
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn parse(input: &str) -> Result<JsonValue, String> {
    let mut p = Parser::new(input);
    let value = p.parse_value()?;
    p.skip_ws();
    if p.pos < p.input.len() {
        return Err(format!(
            "trailing data after JSON value at pos {}",
            p.pos
        ));
    }
    Ok(value)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_null() {
        assert_eq!(parse("null").unwrap(), JsonValue::Null);
        assert_eq!(parse("  null  ").unwrap(), JsonValue::Null);
    }

    #[test]
    fn test_parse_bool() {
        assert_eq!(parse("true").unwrap(), JsonValue::Bool(true));
        assert_eq!(parse("false").unwrap(), JsonValue::Bool(false));
        assert_eq!(parse("  true  ").unwrap(), JsonValue::Bool(true));
    }

    #[test]
    fn test_parse_number() {
        assert_eq!(parse("42").unwrap(), JsonValue::Number(42.0));
        assert_eq!(parse("-3.14").unwrap(), JsonValue::Number(-3.14));
        assert_eq!(parse("1e3").unwrap(), JsonValue::Number(1000.0));
        assert_eq!(parse("0").unwrap(), JsonValue::Number(0.0));
        assert_eq!(parse("1.5E2").unwrap(), JsonValue::Number(150.0));
    }

    #[test]
    fn test_parse_string() {
        assert_eq!(
            parse(r#""hello""#).unwrap(),
            JsonValue::Str("hello".to_string())
        );
        // \n escape
        assert_eq!(
            parse(r#""line1\nline2""#).unwrap(),
            JsonValue::Str("line1\nline2".to_string())
        );
        // \" escape
        assert_eq!(
            parse(r#""say \"hi\"""#).unwrap(),
            JsonValue::Str(r#"say "hi""#.to_string())
        );
        // \\ escape
        assert_eq!(
            parse(r#""back\\slash""#).unwrap(),
            JsonValue::Str("back\\slash".to_string())
        );
    }

    #[test]
    fn test_parse_array() {
        let v = parse("[1, 2, 3]").unwrap();
        assert_eq!(
            v,
            JsonValue::Array(vec![
                JsonValue::Number(1.0),
                JsonValue::Number(2.0),
                JsonValue::Number(3.0),
            ])
        );
    }

    #[test]
    fn test_parse_object() {
        let v = parse(r#"{"a": 1, "b": true}"#).unwrap();
        assert_eq!(
            v,
            JsonValue::Object(vec![
                ("a".to_string(), JsonValue::Number(1.0)),
                ("b".to_string(), JsonValue::Bool(true)),
            ])
        );
        assert_eq!(v.get("a"), Some(&JsonValue::Number(1.0)));
        assert_eq!(v.get("b"), Some(&JsonValue::Bool(true)));
        assert_eq!(v.get("c"), None);
    }

    #[test]
    fn test_nested_get_path() {
        let v = parse(r#"{"outer": {"inner": 42}}"#).unwrap();
        assert_eq!(
            v.get_path(&["outer", "inner"]),
            Some(&JsonValue::Number(42.0))
        );
        assert_eq!(v.get_path(&["outer"]), Some(&JsonValue::Object(vec![
            ("inner".to_string(), JsonValue::Number(42.0)),
        ])));
        assert_eq!(v.get_path(&["outer", "missing"]), None);
        assert_eq!(v.get_path(&[]), Some(&v));
    }

    #[test]
    fn test_roundtrip() {
        let inputs = [
            "null",
            "true",
            "false",
            "42",
            "-3.14",
            r#""hello world""#,
            r#"[1,2,3]"#,
            r#"{"key":"value","num":99}"#,
        ];
        for input in &inputs {
            let parsed = parse(input).expect(input);
            let serialized = parsed.to_json_string();
            let reparsed = parse(&serialized).expect(&serialized);
            assert_eq!(parsed, reparsed, "roundtrip failed for: {}", input);
        }
    }

    #[test]
    fn test_parse_empty_structures() {
        assert_eq!(parse("[]").unwrap(), JsonValue::Array(vec![]));
        assert_eq!(parse("{}").unwrap(), JsonValue::Object(vec![]));
    }

    #[test]
    fn test_unicode_escape() {
        // \u0041 is 'A'
        let v = parse(r#""\u0041""#).unwrap();
        assert_eq!(v, JsonValue::Str("A".to_string()));

        // \u00e9 is 'é'
        let v = parse(r#""\u00e9""#).unwrap();
        assert_eq!(v, JsonValue::Str("é".to_string()));
    }
}
