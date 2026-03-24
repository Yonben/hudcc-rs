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

struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Parser {
            input: input.as_bytes(),
            pos: 0,
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.input.len() {
            match self.input[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let b = self.input.get(self.pos).copied();
        if b.is_some() {
            self.pos += 1;
        }
        b
    }

    fn expect(&mut self, ch: u8) -> Result<(), String> {
        match self.advance() {
            Some(c) if c == ch => Ok(()),
            Some(c) => Err(format!(
                "expected '{}' but got '{}' at pos {}",
                ch as char, c as char, self.pos - 1
            )),
            None => Err(format!(
                "expected '{}' but reached end of input",
                ch as char
            )),
        }
    }

    fn parse_value(&mut self) -> Result<JsonValue, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'"') => self.parse_string().map(JsonValue::Str),
            Some(b't') | Some(b'f') => self.parse_bool(),
            Some(b'n') => self.parse_null(),
            Some(b'[') => self.parse_array(),
            Some(b'{') => self.parse_object(),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.parse_number(),
            Some(c) => Err(format!(
                "unexpected character '{}' at pos {}",
                c as char, self.pos
            )),
            None => Err("unexpected end of input".to_string()),
        }
    }

    fn parse_null(&mut self) -> Result<JsonValue, String> {
        self.consume_literal(b"null")?;
        Ok(JsonValue::Null)
    }

    fn parse_bool(&mut self) -> Result<JsonValue, String> {
        if self.peek() == Some(b't') {
            self.consume_literal(b"true")?;
            Ok(JsonValue::Bool(true))
        } else {
            self.consume_literal(b"false")?;
            Ok(JsonValue::Bool(false))
        }
    }

    fn consume_literal(&mut self, lit: &[u8]) -> Result<(), String> {
        for &expected in lit {
            match self.advance() {
                Some(c) if c == expected => {}
                Some(c) => {
                    return Err(format!(
                        "expected '{}' while parsing literal '{}', got '{}' at pos {}",
                        expected as char,
                        std::str::from_utf8(lit).unwrap_or("?"),
                        c as char,
                        self.pos - 1
                    ))
                }
                None => {
                    return Err(format!(
                        "unexpected end of input while parsing literal '{}'",
                        std::str::from_utf8(lit).unwrap_or("?")
                    ))
                }
            }
        }
        Ok(())
    }

    fn parse_number(&mut self) -> Result<JsonValue, String> {
        let start = self.pos;
        // Optional minus
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        // Integer part
        if self.peek() == Some(b'0') {
            self.pos += 1;
        } else if matches!(self.peek(), Some(b'1'..=b'9')) {
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        } else {
            return Err(format!("invalid number at pos {}", self.pos));
        }
        // Fractional part
        if self.peek() == Some(b'.') {
            self.pos += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(format!("expected digit after '.' at pos {}", self.pos));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        // Exponent part
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.pos += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(format!("expected digit in exponent at pos {}", self.pos));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        let raw = std::str::from_utf8(&self.input[start..self.pos])
            .map_err(|e| format!("invalid UTF-8 in number: {}", e))?;
        raw.parse::<f64>()
            .map(JsonValue::Number)
            .map_err(|e| format!("invalid number '{}': {}", raw, e))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut s = String::new();
        loop {
            match self.advance() {
                None => return Err("unterminated string".to_string()),
                Some(b'"') => break,
                Some(b'\\') => {
                    match self.advance() {
                        Some(b'"') => s.push('"'),
                        Some(b'\\') => s.push('\\'),
                        Some(b'/') => s.push('/'),
                        Some(b'n') => s.push('\n'),
                        Some(b't') => s.push('\t'),
                        Some(b'r') => s.push('\r'),
                        Some(b'b') => s.push('\x08'),
                        Some(b'f') => s.push('\x0C'),
                        Some(b'u') => {
                            // Read exactly 4 hex digits.
                            let mut hex = String::with_capacity(4);
                            for _ in 0..4 {
                                match self.advance() {
                                    Some(h) if (h as char).is_ascii_hexdigit() => {
                                        hex.push(h as char)
                                    }
                                    Some(c) => {
                                        return Err(format!(
                                            "invalid hex digit '{}' in \\uXXXX escape",
                                            c as char
                                        ))
                                    }
                                    None => {
                                        return Err(
                                            "unexpected end of input in \\uXXXX escape"
                                                .to_string(),
                                        )
                                    }
                                }
                            }
                            let code = u32::from_str_radix(&hex, 16).map_err(|e| {
                                format!("invalid unicode escape \\u{}: {}", hex, e)
                            })?;
                            let ch = char::from_u32(code).ok_or_else(|| {
                                format!("invalid unicode code point U+{:04X}", code)
                            })?;
                            s.push(ch);
                        }
                        Some(c) => {
                            return Err(format!(
                                "invalid escape sequence '\\{}'",
                                c as char
                            ))
                        }
                        None => {
                            return Err("unexpected end of input after '\\'".to_string())
                        }
                    }
                }
                Some(b) if b >= 0x80 => {
                    // Multi-byte UTF-8 sequence. Determine length from leading byte.
                    let byte_len = if b & 0xE0 == 0xC0 {
                        2
                    } else if b & 0xF0 == 0xE0 {
                        3
                    } else if b & 0xF8 == 0xF0 {
                        4
                    } else {
                        return Err(format!(
                            "invalid UTF-8 leading byte 0x{:02X} at pos {}",
                            b,
                            self.pos - 1
                        ));
                    };
                    // pos already advanced past the leading byte
                    let start = self.pos - 1;
                    let end = start + byte_len;
                    if end > self.input.len() {
                        return Err(format!(
                            "truncated UTF-8 sequence at pos {}",
                            start
                        ));
                    }
                    let slice = &self.input[start..end];
                    let utf8 = std::str::from_utf8(slice).map_err(|e| {
                        format!("invalid UTF-8 sequence at pos {}: {}", start, e)
                    })?;
                    s.push_str(utf8);
                    self.pos = end;
                }
                Some(b) => s.push(b as char),
            }
        }
        Ok(s)
    }

    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.expect(b'[')?;
        self.skip_ws();
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(JsonValue::Array(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    break;
                }
                Some(c) => {
                    return Err(format!(
                        "expected ',' or ']' in array, got '{}'",
                        c as char
                    ))
                }
                None => return Err("unexpected end of input in array".to_string()),
            }
        }
        Ok(JsonValue::Array(items))
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.expect(b'{')?;
        self.skip_ws();
        let mut pairs = Vec::new();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(JsonValue::Object(pairs));
        }
        loop {
            self.skip_ws();
            // Key must be a string.
            if self.peek() != Some(b'"') {
                return Err(format!(
                    "expected string key in object, got {:?} at pos {}",
                    self.peek().map(|b| b as char),
                    self.pos
                ));
            }
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(b':')?;
            let value = self.parse_value()?;
            pairs.push((key, value));
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    break;
                }
                Some(c) => {
                    return Err(format!(
                        "expected ',' or '}}' in object, got '{}'",
                        c as char
                    ))
                }
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
// Reference char-based parser (kept for differential testing)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod char_parser {
    use super::JsonValue;

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
                Some(c) => Err(format!(
                    "expected '{}' but got '{}' at pos {}",
                    ch,
                    c,
                    self.pos - 1
                )),
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
                            ch,
                            lit,
                            c,
                            self.pos - 1
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
            if self.peek() == Some('-') {
                self.pos += 1;
            }
            if self.peek() == Some('0') {
                self.pos += 1;
            } else if matches!(self.peek(), Some('1'..='9')) {
                while matches!(self.peek(), Some('0'..='9')) {
                    self.pos += 1;
                }
            } else {
                return Err(format!("invalid number at pos {}", self.pos));
            }
            if self.peek() == Some('.') {
                self.pos += 1;
                if !matches!(self.peek(), Some('0'..='9')) {
                    return Err(format!("expected digit after '.' at pos {}", self.pos));
                }
                while matches!(self.peek(), Some('0'..='9')) {
                    self.pos += 1;
                }
            }
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
                    Some('\\') => match self.advance() {
                        Some('"') => s.push('"'),
                        Some('\\') => s.push('\\'),
                        Some('/') => s.push('/'),
                        Some('n') => s.push('\n'),
                        Some('t') => s.push('\t'),
                        Some('r') => s.push('\r'),
                        Some('b') => s.push('\x08'),
                        Some('f') => s.push('\x0C'),
                        Some('u') => {
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
                                            "unexpected end of input in \\uXXXX escape"
                                                .to_string(),
                                        )
                                    }
                                }
                            }
                            let code = u32::from_str_radix(&hex, 16).map_err(|e| {
                                format!("invalid unicode escape \\u{}: {}", hex, e)
                            })?;
                            let ch = char::from_u32(code).ok_or_else(|| {
                                format!("invalid unicode code point U+{:04X}", code)
                            })?;
                            s.push(ch);
                        }
                        Some(c) => return Err(format!("invalid escape sequence '\\{}'", c)),
                        None => return Err("unexpected end of input after '\\'".to_string()),
                    },
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
                    Some(c) => {
                        return Err(format!("expected ',' or ']' in array, got '{}'", c))
                    }
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
                    Some(c) => {
                        return Err(format!("expected ',' or '}}' in object, got '{}'", c))
                    }
                    None => return Err("unexpected end of input in object".to_string()),
                }
            }
            Ok(JsonValue::Object(pairs))
        }
    }

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
    fn test_deeply_nested() {
        let mut json = String::new();
        for _ in 0..15 {
            json.push_str(r#"{"a":"#);
        }
        json.push_str("1");
        for _ in 0..15 {
            json.push('}');
        }
        let val = parse(&json).unwrap();
        let keys = vec!["a"; 14];
        let deepest = val.get_path(&keys).unwrap();
        assert_eq!(deepest.get("a"), Some(&JsonValue::Number(1.0)));
    }

    #[test]
    fn test_very_long_string() {
        let long = "x".repeat(10_000);
        let json = format!(r#""{}""#, long);
        let val = parse(&json).unwrap();
        assert_eq!(val.as_str().unwrap().len(), 10_000);
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

    #[test]
    fn test_byte_parser_matches_char_parser() {
        let long_string = format!(r#""{}""#, "abcdef".repeat(1000));
        let cases: Vec<&str> = vec![
            "null",
            "true",
            "false",
            "42",
            "-3.14",
            "1e10",
            r#""hello""#,
            r#""escaped \"quotes\"""#,
            r#""unicode \u00e9""#,
            r#""multi-byte: café ñ 日本語""#,
            "[]",
            "{}",
            "[1, 2, 3]",
            r#"{"a": 1, "b": true, "c": null}"#,
            r#"{"nested": {"deep": {"value": [1, "two", false]}}}"#,
            r#"[{"id": 1}, {"id": 2}]"#,
            r#""line1\nline2\ttab""#,
            r#""back\\slash""#,
            "  { \"spaced\" :  42  }  ",
            &long_string,
        ];

        for input in &cases {
            let byte_result = parse(input);
            let char_result = super::char_parser::parse(input);
            assert_eq!(
                byte_result, char_result,
                "parsers disagree on input: {}",
                if input.len() > 100 {
                    &input[..100]
                } else {
                    input
                }
            );
        }
    }
}
