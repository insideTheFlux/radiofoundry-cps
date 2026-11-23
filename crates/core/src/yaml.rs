// SPDX-License-Identifier: MIT
// Copyright (c) 2026 The RadioFoundry Authors
// Copyright (c) 2026 insideTheFlux
//
// The YAML subset a qdmr codeplug file uses (RadioFoundry's docs/spec/qdmr-yaml.md 2): block maps and lists, flow
// maps and lists, plain, single- and double-quoted scalars, literal and folded block scalars, comments, document
// markers and local tags (!default). Anchors, aliases, complex keys and multiple documents are refused. Ported from
// RadioFoundry's `site/static/js/codeplug/yaml.js`, which was written from the YAML 1.2 specification.

/// A YAML value. A map keeps its keys in file order.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<Value>),
    Map(Vec<(String, Value)>),
    /// `!default ""`: a local tag and its value.
    Tag(String, Box<Value>),
    /// A list or map the writer puts in flow style (`{a: 1}`, `[a, b]`).
    Flow(Box<Value>),
    /// A scalar the writer emits as given, such as `0x293` for a number best read in hex. It must read back as the value.
    Raw(String),
}

impl Value {
    pub fn str(s: impl Into<String>) -> Value {
        Value::Str(s.into())
    }

    pub fn flow(v: Value) -> Value {
        Value::Flow(Box::new(v))
    }

    pub fn tag(t: &str, v: Value) -> Value {
        Value::Tag(t.to_string(), Box::new(v))
    }

    /// A map entry, if this is a map (a tagged map is looked into too).
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self.untag() {
            Value::Map(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The value without its tag.
    pub fn untag(&self) -> &Value {
        match self {
            Value::Tag(_, v) => v.untag(),
            v => v,
        }
    }

    pub fn is_default_tag(&self) -> bool {
        matches!(self, Value::Tag(t, _) if t == "!default")
    }

    pub fn as_list(&self) -> &[Value] {
        match self.untag() {
            Value::List(l) => l,
            _ => &[],
        }
    }

    pub fn is_map(&self) -> bool {
        matches!(self.untag(), Value::Map(_))
    }

    /// JavaScript's truthiness, which the qdmr checks were written with.
    pub fn truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0 && !f.is_nan(),
            Value::Str(s) => !s.is_empty(),
            _ => true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YamlError {
    pub message: String,
    pub line: usize,
}

impl std::fmt::Display for YamlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line > 0 {
            write!(f, "line {}: {}", self.line, self.message)
        } else {
            f.write_str(&self.message)
        }
    }
}

fn err<T>(message: &str, line: usize) -> Result<T, YamlError> {
    Err(YamlError {
        message: message.to_string(),
        line,
    })
}

// --- Parser ---

#[derive(Clone)]
struct Line {
    /// -1 for a blank line.
    indent: i32,
    text: String,
    raw: String,
    line: usize,
}

fn is_space(c: char) -> bool {
    c.is_whitespace()
}

/// The line without its comment: # after whitespace (or at the start) and outside quotes.
fn strip_comment(s: &str) -> String {
    let ch: Vec<char> = s.chars().collect();
    let mut q: Option<char> = None;
    let mut i = 0;
    while i < ch.len() {
        let c = ch[i];
        if let Some(quote) = q {
            if c == '\\' && quote == '"' {
                i += 1;
            } else if c == quote {
                if quote == '\'' && ch.get(i + 1) == Some(&'\'') {
                    i += 1;
                } else {
                    q = None;
                }
            }
        } else if c == '"' || c == '\'' {
            if i == 0 || is_space(ch[i - 1]) || "[{,:-".contains(ch[i - 1]) {
                q = Some(c);
            }
        } else if c == '#' && (i == 0 || is_space(ch[i - 1])) {
            return ch[..i].iter().collect::<String>().trim_end().to_string();
        }
        i += 1;
    }
    s.trim_end().to_string()
}

fn lines(text: &str) -> Result<Vec<Line>, YamlError> {
    let mut out: Vec<Line> = Vec::new();
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    for (i, raw) in text.split('\n').enumerate() {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        let n = i + 1;
        if raw.chars().take_while(|c| is_space(*c)).any(|c| c == '\t') {
            return err("tab in indentation", n);
        }
        let body = strip_comment(raw);
        let t = body.trim();
        if t.is_empty() {
            out.push(Line {
                indent: -1,
                text: String::new(),
                raw: raw.to_string(),
                line: n,
            });
            continue;
        }
        let marker = (body.starts_with("---") || body.starts_with("..."))
            && body[3..].chars().next().is_none_or(is_space);
        if marker {
            if t.starts_with("...") {
                break;
            }
            if out.iter().any(|l| l.indent >= 0) {
                return err("more than one document", n);
            }
            let rest = t[3..].trim();
            if !rest.is_empty() {
                out.push(Line {
                    indent: 0,
                    text: rest.to_string(),
                    raw: raw.to_string(),
                    line: n,
                });
            }
            continue;
        }
        if t.starts_with('%') {
            continue; // a directive
        }
        let indent = (body.chars().count() - body.trim_start().chars().count()) as i32;
        out.push(Line {
            indent,
            text: t.to_string(),
            raw: raw.to_string(),
            line: n,
        });
    }
    Ok(out)
}

/// Parses one YAML document; `Null` for an empty one.
pub fn parse(text: &str) -> Result<Value, YamlError> {
    let mut p = BlockParser {
        ls: lines(text)?,
        i: 0,
    };
    p.skip_blank();
    if p.done() {
        return Ok(Value::Null);
    }
    let indent = p.cur().indent;
    let v = p.node(indent)?;
    p.skip_blank();
    if !p.done() {
        return err("unexpected text", p.cur().line);
    }
    Ok(v)
}

struct BlockParser {
    ls: Vec<Line>,
    i: usize,
}

fn is_seq_item(t: &str) -> bool {
    t == "-" || t.starts_with("- ")
}

impl BlockParser {
    fn done(&self) -> bool {
        self.i >= self.ls.len()
    }

    fn cur(&self) -> &Line {
        &self.ls[self.i]
    }

    fn skip_blank(&mut self) {
        while !self.done() && self.cur().indent < 0 {
            self.i += 1;
        }
    }

    fn node(&mut self, indent: i32) -> Result<Value, YamlError> {
        let l = self.cur().clone();
        if is_seq_item(&l.text) {
            return self.seq(indent);
        }
        if key_of(&l.text, l.line)?.is_some() {
            return self.map(indent);
        }
        self.i += 1;
        self.inline_rest(&l.text, &l, indent)
    }

    fn seq(&mut self, indent: i32) -> Result<Value, YamlError> {
        let mut out = Vec::new();
        loop {
            self.skip_blank();
            if self.done() {
                break;
            }
            let l = self.cur().clone();
            if l.indent != indent || !is_seq_item(&l.text) {
                if l.indent > indent {
                    return err("bad indentation", l.line);
                }
                break;
            }
            let rest = l.text[1..].trim_start().to_string();
            if rest.is_empty() {
                self.i += 1;
                out.push(self.child(indent)?);
            } else {
                // "- key: value" opens a map whose keys line up with "key"; "- - x" a nested list
                let inner = l.indent + (l.text.chars().count() - rest.chars().count()) as i32;
                if is_seq_item(&rest) || key_of(&rest, l.line)?.is_some() {
                    self.ls[self.i] = Line {
                        indent: inner,
                        text: rest,
                        ..l
                    };
                    out.push(self.node(inner)?);
                } else {
                    self.i += 1;
                    out.push(self.inline_rest(&rest, &l, indent)?);
                }
            }
        }
        Ok(Value::List(out))
    }

    fn map(&mut self, indent: i32) -> Result<Value, YamlError> {
        let mut out: Vec<(String, Value)> = Vec::new();
        loop {
            self.skip_blank();
            if self.done() {
                break;
            }
            let l = self.cur().clone();
            if l.indent != indent {
                if l.indent > indent {
                    return err("bad indentation", l.line);
                }
                break;
            }
            if is_seq_item(&l.text) {
                break;
            }
            let Some((key, rest)) = key_of(&l.text, l.line)? else {
                return err("expected a key", l.line);
            };
            if out.iter().any(|(k, _)| *k == key) {
                return err(&format!("duplicate key {key}"), l.line);
            }
            self.i += 1;
            let v = if rest.is_empty() {
                // the value is on the following lines: deeper, or a list at the same indent
                self.skip_blank();
                if !self.done() && self.cur().indent == indent && is_seq_item(&self.cur().text) {
                    self.seq(indent)?
                } else {
                    self.child(indent)?
                }
            } else {
                self.inline_rest(&rest, &l, indent)?
            };
            out.push((key, v));
        }
        Ok(Value::Map(out))
    }

    /// The block node below a line at indent, or null if none follows.
    fn child(&mut self, indent: i32) -> Result<Value, YamlError> {
        self.skip_blank();
        if self.done() || self.cur().indent <= indent {
            return Ok(Value::Null);
        }
        let i = self.cur().indent;
        self.node(i)
    }

    /// An inline value; a block scalar or a flow collection may continue on the following lines.
    fn inline_rest(&mut self, text: &str, l: &Line, indent: i32) -> Result<Value, YamlError> {
        let mut text = text.to_string();
        let mut tag: Option<String> = None;
        if text.starts_with('!') {
            let end = text
                .char_indices()
                .skip(1)
                .find(|(_, c)| !(c.is_alphanumeric() || *c == '_' || *c == '!' || *c == '-'))
                .map_or(text.len(), |(i, _)| i);
            let after = &text[end..];
            if after.is_empty() || after.starts_with(is_space) {
                tag = Some(text[..end].to_string());
                text = after.trim_start().to_string();
            }
        }
        if text.starts_with('&') || text.starts_with('*') {
            return err("anchors and aliases are not supported", l.line);
        }
        let v = if is_block_head(&text) {
            Value::Str(self.block_scalar(&text, indent))
        } else if text.is_empty() && tag.is_some() {
            self.child(indent)?
        } else {
            // a flow collection or a quoted scalar left open continues on the next lines
            while is_open(&text) && !self.done() {
                let n = self.cur().clone();
                self.i += 1;
                if n.indent >= 0 {
                    text.push(' ');
                    text.push_str(&n.text);
                }
            }
            let mut f = FlowParser::new(&text, l.line);
            let v = f.value(false)?;
            f.end()?;
            v
        };
        Ok(match tag {
            Some(t) => Value::Tag(t, Box::new(v)),
            None => v,
        })
    }

    fn block_scalar(&mut self, head: &str, indent: i32) -> String {
        let folded = head.starts_with('>');
        let strip = head.contains('-');
        let keep = !strip && head.contains('+');
        let mut body: Vec<String> = Vec::new();
        let mut ind: Option<usize> = None;
        while !self.done() {
            let l = self.cur().clone();
            let lead = l.raw.chars().count() - l.raw.trim_start().chars().count();
            let blank = l.raw.trim().is_empty();
            if !blank && lead as i32 <= indent {
                break;
            }
            if ind.is_none() && !blank {
                ind = Some(lead);
            }
            body.push(if blank {
                String::new()
            } else {
                l.raw.chars().skip(ind.unwrap_or(lead)).collect()
            });
            self.i += 1;
        }
        let mut s = String::new();
        if folded {
            for (k, x) in body.iter().enumerate() {
                if k == 0 {
                    s = x.clone();
                } else if x.is_empty() || body[k - 1].is_empty() {
                    s.push('\n');
                    s.push_str(x);
                } else {
                    s.push(' ');
                    s.push_str(x);
                }
            }
        } else {
            s = body.join("\n");
        }
        let mut s = s.trim_end_matches('\n').to_string();
        if !strip && !keep && !s.is_empty() {
            s.push('\n');
        }
        if keep {
            s.push('\n');
        }
        s
    }
}

fn is_block_head(t: &str) -> bool {
    let mut c = t.chars();
    matches!(c.next(), Some('|' | '>')) && c.all(|x| x == '-' || x == '+' || x.is_ascii_digit())
}

/// `(key, rest)` if the text starts a map entry ("key: value" or "key:"), else `None`.
fn key_of(text: &str, line: usize) -> Result<Option<(String, String)>, YamlError> {
    if text.starts_with("? ") {
        return err("complex keys are not supported", line);
    }
    let ch: Vec<char> = text.chars().collect();
    if ch[0] == '{' || ch[0] == '[' {
        return Ok(None);
    }
    let (key, i);
    if ch[0] == '"' || ch[0] == '\'' {
        let mut f = FlowParser::new(text, line);
        key = f.quoted()?;
        i = f.i;
    } else {
        // the text before the first ':' that is followed by whitespace or the end; # or an earlier ':' ends the search
        let mut j = 0;
        while j < ch.len() && ch[j] != ':' && ch[j] != '#' {
            j += 1;
        }
        if j >= ch.len() || ch[j] != ':' || !(j + 1 >= ch.len() || is_space(ch[j + 1])) {
            return Ok(None);
        }
        let m: String = ch[..j].iter().collect();
        if m.is_empty() {
            return Ok(None);
        }
        let first = m.chars().next().unwrap();
        let second_word = m.chars().nth(1).is_some_and(|c| c.is_alphanumeric() || c == '_');
        if "-?|>!&*%@`".contains(first) && !(first == '-' && second_word) {
            return Ok(None);
        }
        key = m.trim().to_string();
        i = j;
    }
    if ch.get(i) != Some(&':') || (i + 1 < ch.len() && !is_space(ch[i + 1])) {
        return Ok(None);
    }
    let rest: String = ch[i + 1..].iter().collect();
    Ok(Some((key, rest.trim().to_string())))
}

/// True while a flow collection or a quoted scalar in the text isn't closed.
fn is_open(text: &str) -> bool {
    let ch: Vec<char> = text.chars().collect();
    let (mut depth, mut q): (i32, Option<char>) = (0, None);
    let mut i = 0;
    while i < ch.len() {
        let c = ch[i];
        if let Some(quote) = q {
            if c == '\\' && quote == '"' {
                i += 1;
            } else if c == quote {
                if quote == '\'' && ch.get(i + 1) == Some(&'\'') {
                    i += 1;
                } else {
                    q = None;
                }
            }
        } else if (c == '"' || c == '\'')
            && (i == 0 || is_space(ch[i - 1]) || "[{,:".contains(ch[i - 1]))
        {
            q = Some(c);
        } else if c == '[' || c == '{' {
            depth += 1;
        } else if c == ']' || c == '}' {
            depth -= 1;
        }
        i += 1;
    }
    q.is_some() || depth > 0
}

struct FlowParser {
    s: Vec<char>,
    i: usize,
    line: usize,
}

impl FlowParser {
    fn new(s: &str, line: usize) -> Self {
        FlowParser {
            s: s.chars().collect(),
            i: 0,
            line,
        }
    }

    fn ws(&mut self) {
        while self.i < self.s.len() && is_space(self.s[self.i]) {
            self.i += 1;
        }
    }

    fn end(&mut self) -> Result<(), YamlError> {
        self.ws();
        if self.i < self.s.len() {
            let rest: String = self.s[self.i..].iter().take(10).collect();
            return err(&format!("unexpected \"{rest}\""), self.line);
        }
        Ok(())
    }

    /// One value; `in_flow`: inside [] or {}, where , ] } end a plain scalar.
    fn value(&mut self, in_flow: bool) -> Result<Value, YamlError> {
        self.ws();
        let mut tag: Option<String> = None;
        if self.s.get(self.i) == Some(&'!') {
            let start = self.i;
            self.i += 1;
            while self.i < self.s.len()
                && (self.s[self.i].is_alphanumeric() || "_!-".contains(self.s[self.i]))
            {
                self.i += 1;
            }
            tag = Some(self.s[start..self.i].iter().collect());
            self.ws();
        }
        let c = self.s.get(self.i).copied();
        let v = match c {
            Some('&' | '*') => return err("anchors and aliases are not supported", self.line),
            Some('[') => self.list()?,
            Some('{') => self.mapping()?,
            Some('"' | '\'') => Value::Str(self.quoted()?),
            _ => self.plain(in_flow, false),
        };
        Ok(match tag {
            Some(t) => Value::Tag(t, Box::new(v)),
            None => v,
        })
    }

    fn list(&mut self) -> Result<Value, YamlError> {
        let mut out = Vec::new();
        self.i += 1;
        loop {
            self.ws();
            if self.s.get(self.i) == Some(&']') {
                self.i += 1;
                return Ok(Value::List(out));
            }
            out.push(self.value(true)?);
            self.ws();
            match self.s.get(self.i) {
                Some(',') => self.i += 1,
                Some(']') => {}
                _ => return err("expected , or ]", self.line),
            }
        }
    }

    fn mapping(&mut self) -> Result<Value, YamlError> {
        let mut out: Vec<(String, Value)> = Vec::new();
        self.i += 1;
        loop {
            self.ws();
            if self.s.get(self.i) == Some(&'}') {
                self.i += 1;
                return Ok(Value::Map(out));
            }
            let k = match self.s.get(self.i) {
                Some('"' | '\'') => self.quoted()?,
                _ => text_of(&self.plain(true, true)),
            };
            self.ws();
            let mut v = Value::Null;
            if self.s.get(self.i) == Some(&':') {
                self.i += 1;
                v = self.value(true)?;
            }
            match out.iter_mut().find(|(key, _)| *key == k) {
                Some(e) => e.1 = v,
                None => out.push((k, v)),
            }
            self.ws();
            match self.s.get(self.i) {
                Some(',') => self.i += 1,
                Some('}') => {}
                _ => return err("expected , or }", self.line),
            }
        }
    }

    fn quoted(&mut self) -> Result<String, YamlError> {
        let q = self.s[self.i];
        self.i += 1;
        let mut out = String::new();
        loop {
            if self.i >= self.s.len() {
                return err("unclosed quote", self.line);
            }
            let c = self.s[self.i];
            self.i += 1;
            if c == q {
                if q == '\'' && self.s.get(self.i) == Some(&'\'') {
                    out.push('\'');
                    self.i += 1;
                    continue;
                }
                return Ok(out);
            }
            if c == '\\' && q == '"' {
                out.push(self.escape()?);
            } else {
                out.push(c);
            }
        }
    }

    fn escape(&mut self) -> Result<char, YamlError> {
        let Some(&c) = self.s.get(self.i) else {
            return err("unclosed quote", self.line);
        };
        self.i += 1;
        let simple = match c {
            '0' => Some('\0'),
            'a' => Some('\x07'),
            'b' => Some('\x08'),
            't' | '\t' => Some('\t'),
            'n' => Some('\n'),
            'v' => Some('\x0b'),
            'f' => Some('\x0c'),
            'r' => Some('\r'),
            'e' => Some('\x1b'),
            ' ' => Some(' '),
            '"' => Some('"'),
            '/' => Some('/'),
            '\\' => Some('\\'),
            'N' => Some('\u{85}'),
            '_' => Some('\u{a0}'),
            'L' => Some('\u{2028}'),
            'P' => Some('\u{2029}'),
            _ => None,
        };
        if let Some(s) = simple {
            return Ok(s);
        }
        let len = match c {
            'x' => 2,
            'u' => 4,
            'U' => 8,
            _ => return err(&format!("unknown escape \\{c}"), self.line),
        };
        let hex: String = self.s[self.i..(self.i + len).min(self.s.len())]
            .iter()
            .collect();
        if hex.len() != len || !hex.chars().all(|h| h.is_ascii_hexdigit()) {
            return err("bad escape", self.line);
        }
        self.i += len;
        match char::from_u32(u32::from_str_radix(&hex, 16).unwrap_or(0xFFFD)) {
            Some(ch) => Ok(ch),
            None => err("bad escape", self.line),
        }
    }

    /// A plain scalar, resolved to null, a boolean, a number or a string; `as_key`: stops at ':' too.
    fn plain(&mut self, in_flow: bool, as_key: bool) -> Value {
        let start = self.i;
        while self.i < self.s.len() {
            let c = self.s[self.i];
            if in_flow && (c == ',' || c == ']' || c == '}') {
                break;
            }
            if c == ':'
                && (as_key || in_flow)
                && (self.i + 1 >= self.s.len()
                    || is_space(self.s[self.i + 1])
                    || ",]}".contains(self.s[self.i + 1]))
            {
                break;
            }
            self.i += 1;
        }
        let t: String = self.s[start..self.i].iter().collect();
        resolve(t.trim())
    }
}

/// A scalar as the text YAML would show: what JavaScript's String() gives for the values a plain scalar resolves to.
pub fn text_of(v: &Value) -> String {
    match v.untag() {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => float_text(*f),
        Value::Str(s) => s.clone(),
        _ => String::new(),
    }
}

fn float_text(f: f64) -> String {
    if f.is_nan() {
        ".nan".into()
    } else {
        format!("{f}")
    }
}

fn is_int(s: &str) -> bool {
    let d = s.strip_prefix(['-', '+']).unwrap_or(s);
    !d.is_empty() && d.chars().all(|c| c.is_ascii_digit())
}

fn is_float(s: &str) -> bool {
    // [-+]?(\.\d+|\d+(\.\d*)?)([eE][-+]?\d+)?
    let s = s.strip_prefix(['-', '+']).unwrap_or(s);
    let (mant, exp) = match s.find(['e', 'E']) {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    };
    if let Some(e) = exp {
        if !is_int(e) {
            return false;
        }
    }
    let digits = |x: &str| !x.is_empty() && x.chars().all(|c| c.is_ascii_digit());
    match mant.split_once('.') {
        Some((a, b)) => (a.is_empty() && digits(b)) || (digits(a) && (b.is_empty() || digits(b))),
        None => digits(mant),
    }
}

/// The core schema's resolution of a plain scalar.
pub fn resolve(s: &str) -> Value {
    if s.is_empty() || s == "~" || matches!(s, "null" | "Null" | "NULL") {
        return Value::Null;
    }
    if matches!(s, "true" | "True" | "TRUE") {
        return Value::Bool(true);
    }
    if matches!(s, "false" | "False" | "FALSE") {
        return Value::Bool(false);
    }
    if is_int(s) {
        return match s.parse::<i64>() {
            Ok(i) => Value::Int(i),
            Err(_) => s.parse::<f64>().map_or(Value::Str(s.to_string()), Value::Float),
        };
    }
    if let Some(h) = s.strip_prefix("0x") {
        if !h.is_empty() && h.chars().all(|c| c.is_ascii_hexdigit()) {
            if let Ok(i) = i64::from_str_radix(h, 16) {
                return Value::Int(i);
            }
        }
    }
    if is_float(s) {
        if let Ok(f) = s.parse::<f64>() {
            return Value::Float(f);
        }
    }
    Value::Str(s.to_string())
}

// --- Emitter ---

/// YAML text for a value. Maps and lists are written in block style unless wrapped in `Flow`; tags as `!tag value`.
pub fn stringify(v: &Value) -> String {
    let mut out = Vec::new();
    block(v, 0, &mut out);
    out.join("\n") + "\n"
}

fn is_collection(v: &Value) -> bool {
    matches!(v, Value::List(_) | Value::Map(_))
}

fn non_empty_collection(v: &Value) -> bool {
    match v {
        Value::List(l) => !l.is_empty(),
        Value::Map(m) => !m.is_empty(),
        _ => false,
    }
}

fn block(v: &Value, indent: usize, out: &mut Vec<String>) {
    let pad = " ".repeat(indent);
    match v {
        Value::List(items) => {
            if items.is_empty() {
                out.push(format!("{pad}[]"));
                return;
            }
            for x in items {
                match x {
                    Value::Map(m) if !m.is_empty() => {
                        let mut sub = Vec::new();
                        block(x, indent + 2, &mut sub);
                        let first: String = sub[0].chars().skip(indent + 2).collect();
                        out.push(format!("{pad}- {first}"));
                        out.extend(sub.into_iter().skip(1));
                    }
                    Value::List(l) if !l.is_empty() => {
                        out.push(format!("{pad}-"));
                        block(x, indent + 2, out);
                    }
                    _ => out.push(format!("{pad}- {}", inline(x))),
                }
            }
        }
        Value::Map(m) => {
            if m.is_empty() {
                out.push(format!("{pad}{{}}"));
                return;
            }
            for (k, x) in m {
                if is_collection(x) && non_empty_collection(x) {
                    out.push(format!("{pad}{}:", scalar_text(k)));
                    // a list under a key is indented by two, as qdmr writes it
                    block(x, indent + 2, out);
                } else {
                    out.push(format!("{pad}{}: {}", scalar_text(k), inline(x)));
                }
            }
        }
        _ => out.push(format!("{pad}{}", inline(v))),
    }
}

fn inline(v: &Value) -> String {
    match v {
        Value::Tag(t, x) => format!("{t} {}", inline(x)).trim_end().to_string(),
        Value::Flow(x) => inline(x),
        Value::Raw(t) => t.clone(),
        Value::List(l) => format!(
            "[{}]",
            l.iter().map(inline).collect::<Vec<_>>().join(", ")
        ),
        Value::Map(m) => format!(
            "{{{}}}",
            m.iter()
                .map(|(k, x)| format!("{}: {}", scalar_text(k), inline(x)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Null => "~".into(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => float_text(*f),
        Value::Str(s) => scalar_text(s),
    }
}

fn is_control(c: char) -> bool {
    matches!(c, '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' | '\u{2028}' | '\u{2029}')
}

/// A string, quoted when plain text would read back as something else.
pub fn scalar_text(s: &str) -> String {
    let plain_ok = !s.is_empty()
        && s == s.trim()
        && matches!(resolve(s), Value::Str(ref r) if r == s)
        && !s.starts_with(|c: char| "-?:,[]{}#&*!|>'\"%@`".contains(c))
        && !s.contains(|c: char| ":#,[]{}".contains(c))
        && !s.chars().any(is_control);
    if plain_ok {
        return s.to_string();
    }
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if is_control(c) => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Value {
        parse(s).unwrap()
    }

    fn map(items: Vec<(&str, Value)>) -> Value {
        Value::Map(items.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }

    #[test]
    fn scalars_resolve_like_the_core_schema() {
        assert_eq!(resolve("201"), Value::Int(201));
        assert_eq!(resolve("-3"), Value::Int(-3));
        assert_eq!(resolve("0x1F"), Value::Int(31));
        assert_eq!(resolve("439.5625"), Value::Float(439.5625));
        assert_eq!(resolve(".5"), Value::Float(0.5));
        assert_eq!(resolve("1e3"), Value::Float(1000.0));
        assert_eq!(resolve("true"), Value::Bool(true));
        assert_eq!(resolve("~"), Value::Null);
        assert_eq!(resolve("145 MHz"), Value::str("145 MHz"));
        assert_eq!(resolve("1.2.3"), Value::str("1.2.3"));
    }

    #[test]
    fn block_and_flow_collections() {
        let v = p("---\nversion: 0.15.1\nsettings:\n  defaultID: id1\ncontacts:\n  - dmr: {id: cont1, name: TG 9, number: 9}\n  - dmr:\n      id: cont2\n      name: \"It's\"\nzones:\n- id: z1\n  A: [ch1, ch2]\n  B: []\n...\n");
        assert_eq!(v.get("version"), Some(&Value::str("0.15.1")));
        assert_eq!(
            v.get("settings").and_then(|s| s.get("defaultID")),
            Some(&Value::str("id1"))
        );
        let c = v.get("contacts").unwrap().as_list();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].get("dmr").unwrap().get("number"), Some(&Value::Int(9)));
        assert_eq!(c[0].get("dmr").unwrap().get("name"), Some(&Value::str("TG 9")));
        assert_eq!(c[1].get("dmr").unwrap().get("name"), Some(&Value::str("It's")));
        let z = &v.get("zones").unwrap().as_list()[0];
        assert_eq!(z.get("A").unwrap().as_list().len(), 2);
        assert_eq!(z.get("B").unwrap().as_list().len(), 0);
    }

    #[test]
    fn tags_comments_and_quotes() {
        let v = p("power: !default \"\" # the radio's\nname: 'a # b'\nrx: ~\nt: \"a\\tb\\u00e9\"\n");
        assert_eq!(
            v.get("power"),
            Some(&Value::tag("!default", Value::str("")))
        );
        assert_eq!(v.get("name"), Some(&Value::str("a # b")));
        assert_eq!(v.get("rx"), Some(&Value::Null));
        assert_eq!(v.get("t"), Some(&Value::str("a\tbé")));
    }

    #[test]
    fn block_scalars() {
        let v = p("a: |\n  one\n  two\nb: >-\n  x\n  y\n");
        assert_eq!(v.get("a"), Some(&Value::str("one\ntwo\n")));
        assert_eq!(v.get("b"), Some(&Value::str("x y")));
    }

    #[test]
    fn what_is_not_supported_is_refused() {
        assert!(parse("a: &x 1\n").is_err());
        assert!(parse("a: *x\n").is_err());
        assert!(parse("? a\n: b\n").is_err());
        assert!(parse("a: 1\na: 2\n").is_err());
        assert!(parse("---\na: 1\n---\nb: 2\n").is_err());
        assert!(parse("a:\n\t- b\n").is_err());
        assert!(parse("a: {b: 1\n").is_err());
        assert!(parse("a: 1\n  b: 2\n").is_err());
    }

    #[test]
    fn the_writer_quotes_what_would_read_back_differently() {
        assert_eq!(scalar_text("abc"), "abc");
        assert_eq!(scalar_text("201"), "\"201\"");
        assert_eq!(scalar_text("true"), "\"true\"");
        assert_eq!(scalar_text(""), "\"\"");
        assert_eq!(scalar_text(" a"), "\" a\"");
        assert_eq!(scalar_text("a: b"), "\"a: b\"");
        assert_eq!(scalar_text("a#b"), "\"a#b\"");
        assert_eq!(scalar_text("a\"b"), "a\"b");
        assert_eq!(scalar_text("\"a"), "\"\\\"a\"");
    }

    #[test]
    fn a_document_round_trips_through_the_writer() {
        let doc = map(vec![
            ("version", Value::str("0.15.1")),
            (
                "contacts",
                Value::List(vec![map(vec![(
                    "dmr",
                    Value::flow(map(vec![
                        ("id", Value::str("cont1")),
                        ("name", Value::str("TG 9")),
                        ("number", Value::Int(9)),
                    ])),
                )])]),
            ),
            (
                "channels",
                Value::List(vec![map(vec![(
                    "fm",
                    map(vec![
                        ("id", Value::str("ch1")),
                        ("power", Value::tag("!default", Value::str(""))),
                        ("rxTone", Value::flow(map(vec![("ctcss", Value::str("67 Hz"))]))),
                        ("aprs", Value::Null),
                    ]),
                )])]),
            ),
            ("empty", Value::List(vec![])),
        ]);
        let text = stringify(&doc);
        assert!(text.contains("- dmr: {id: cont1, name: TG 9, number: 9}\n"), "{text}");
        assert!(text.contains("    power: !default \"\"\n"), "{text}");
        assert!(text.contains("empty: []\n"), "{text}");
        assert_eq!(parse(&text).unwrap(), {
            // flow wrappers read back as plain maps
            let mut d = doc.clone();
            fn strip(v: &mut Value) {
                match v {
                    Value::Flow(x) => {
                        let mut inner = (**x).clone();
                        strip(&mut inner);
                        *v = inner;
                    }
                    Value::List(l) => l.iter_mut().for_each(strip),
                    Value::Map(m) => m.iter_mut().for_each(|(_, x)| strip(x)),
                    Value::Tag(_, x) => strip(x),
                    _ => {}
                }
            }
            strip(&mut d);
            d
        });
    }
}
