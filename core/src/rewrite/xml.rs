//! The little of XML that rule programs and tilesets are written in.
//!
//! An element has a name, attributes in quotation marks, and elements
//! inside it. That is all these files use and all that is read: text
//! between elements is skipped (the original's own models have stray
//! letters there, and Russian notes), as are comments, the `<?xml ?>`
//! line and a byte order mark. Each element keeps the line it began on,
//! for messages.
//!
//! A crate was the alternative (`roxmltree` is the small pure-Rust one:
//! https://docs.rs/roxmltree). It would read namespaces, entities and
//! document types that these files never have, and the core has one
//! dependency today. What is needed is under two hundred lines, and a file
//! from a stranger is easier to bound in a reader with nothing in it but
//! what is used: no entity can expand, and nesting is counted.

use super::fault::{within, Expected, Fault, Limits, Thing};

#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub name: String,
    pub line: usize,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Element>,
}

impl Element {
    pub fn get(&self, attribute: &str) -> Option<&str> {
        self.attributes.iter().find(|(name, _)| name == attribute).map(|(_, value)| value.as_str())
    }

    /// The children with one of these names, in the order they are written.
    pub fn elements<'a>(&'a self, names: &'a [&'a str]) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |child| names.contains(&child.name.as_str()))
    }
}

struct Reader<'a> {
    file: &'a str,
    text: &'a [u8],
    at: usize,
    line: usize,
    /// elements read so far, held to the limit on nodes
    count: u64,
    limits: &'a Limits,
}

/// Read a whole file: one element and what is inside it.
pub fn read(file: &str, bytes: &[u8], limits: &Limits) -> Result<Element, Fault> {
    within(Thing::File, bytes.len() as u64, limits.text, file, 0)?;
    let text = std::str::from_utf8(bytes).map_err(|_| Fault::Syntax { file: file.to_string(), line: 0, expected: Expected::Text })?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut reader = Reader { file, text: text.as_bytes(), at: 0, line: 1, count: 0, limits };
    reader.skip_between()?;
    if reader.at >= reader.text.len() {
        return Err(reader.fault(Expected::Element));
    }
    let root = reader.element(0)?;
    reader.skip_between()?;
    if reader.at < reader.text.len() {
        return Err(reader.fault(Expected::OneRoot));
    }
    Ok(root)
}

impl Reader<'_> {
    fn fault(&self, expected: Expected) -> Fault {
        Fault::Syntax { file: self.file.to_string(), line: self.line, expected }
    }

    fn peek(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }

    fn starts(&self, with: &str) -> bool {
        self.text[self.at..].starts_with(with.as_bytes())
    }

    fn step(&mut self, by: usize) {
        for _ in 0..by {
            if self.peek() == Some(b'\n') {
                self.line += 1;
            }
            self.at += 1;
        }
    }

    fn skip_space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.step(1);
        }
    }

    /// Past everything up to `end`, which must come.
    fn skip_past(&mut self, end: &str, expected: Expected) -> Result<(), Fault> {
        while self.at < self.text.len() {
            if self.starts(end) {
                self.step(end.len());
                return Ok(());
            }
            self.step(1);
        }
        Err(self.fault(expected))
    }

    /// Past what lies between elements: text, comments, and `<? ?>` and `<! >` lines.
    fn skip_between(&mut self) -> Result<(), Fault> {
        loop {
            while self.peek().is_some_and(|b| b != b'<') {
                self.step(1);
            }
            if self.starts("<!--") {
                self.skip_past("-->", Expected::EndOfComment)?;
            } else if self.starts("<?") {
                self.skip_past("?>", Expected::EndOfTag)?;
            } else if self.starts("<!") {
                self.skip_past(">", Expected::EndOfTag)?;
            } else {
                return Ok(());
            }
        }
    }

    fn name(&mut self) -> Result<String, Fault> {
        let from = self.at;
        while self.peek().is_some_and(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':') || b >= 0x80) {
            self.at += 1;
        }
        if self.at == from {
            return Err(self.fault(Expected::Name));
        }
        Ok(String::from_utf8_lossy(&self.text[from..self.at]).into_owned())
    }

    /// An attribute's value, with the five named characters and numbered ones put back.
    fn value(&mut self) -> Result<String, Fault> {
        let quote = match self.peek() {
            Some(q @ (b'"' | b'\'')) => q,
            _ => return Err(self.fault(Expected::Quote)),
        };
        self.step(1);
        let from = self.at;
        while self.peek().is_some_and(|b| b != quote) {
            self.step(1);
        }
        if self.peek().is_none() {
            return Err(self.fault(Expected::Quote));
        }
        let raw = String::from_utf8_lossy(&self.text[from..self.at]).into_owned();
        self.step(1);
        if !raw.contains('&') {
            return Ok(raw);
        }
        let mut out = String::with_capacity(raw.len());
        let mut rest = raw.as_str();
        while let Some(amp) = rest.find('&') {
            out.push_str(&rest[..amp]);
            rest = &rest[amp..];
            let Some(end) = rest.find(';') else { break };
            let named = match &rest[1..end] {
                "lt" => Some('<'),
                "gt" => Some('>'),
                "amp" => Some('&'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                other => other.strip_prefix("#x").map(|hex| u32::from_str_radix(hex, 16)).or_else(|| other.strip_prefix('#').map(|dec| dec.parse::<u32>())).and_then(|n| n.ok()).and_then(char::from_u32),
            };
            match named {
                Some(c) => out.push(c),
                None => out.push_str(&rest[..=end]),
            }
            rest = &rest[end + 1..];
        }
        out.push_str(rest);
        Ok(out)
    }

    /// One element, the reader standing at its `<`.
    fn element(&mut self, depth: u64) -> Result<Element, Fault> {
        within(Thing::Depth, depth + 1, self.limits.depth, self.file, self.line)?;
        self.count += 1;
        within(Thing::Nodes, self.count, self.limits.nodes * 8, self.file, self.line)?;
        let line = self.line;
        self.step(1);
        let name = self.name()?;
        let mut element = Element { name, line, attributes: Vec::new(), children: Vec::new() };
        loop {
            self.skip_space();
            match self.peek() {
                Some(b'/') => {
                    self.step(1);
                    if self.peek() != Some(b'>') {
                        return Err(self.fault(Expected::EndOfTag));
                    }
                    self.step(1);
                    return Ok(element);
                }
                Some(b'>') => {
                    self.step(1);
                    break;
                }
                Some(_) => {
                    let attribute = self.name()?;
                    self.skip_space();
                    if self.peek() != Some(b'=') {
                        return Err(self.fault(Expected::Equals));
                    }
                    self.step(1);
                    self.skip_space();
                    let value = self.value()?;
                    element.attributes.push((attribute, value));
                }
                None => return Err(self.fault(Expected::EndOfTag)),
            }
        }
        loop {
            self.skip_between()?;
            if self.at >= self.text.len() {
                return Err(self.fault(Expected::CloseOf));
            }
            if self.starts("</") {
                self.step(2);
                let closing = self.name().map_err(|_| self.fault(Expected::CloseOf))?;
                self.skip_space();
                if closing != element.name || self.peek() != Some(b'>') {
                    return Err(self.fault(Expected::CloseOf));
                }
                self.step(1);
                return Ok(element);
            }
            element.children.push(self.element(depth + 1)?);
        }
    }
}
