//! A saved-data tree, the shape of a Citizens `DataKey`: what an NPC trait reads and writes.
//!
//! Citizens keeps every NPC's traits in one YAML file (`saves.yml`); a trait sees its own subtree
//! (`npc.<id>.traits.sentinel`). The tree here is that subtree, and [`parse_yaml`] / [`to_yaml`]
//! read and write the block-style YAML that SnakeYAML (Bukkit's `YamlConfiguration`) produces,
//! so a Paper server and a Horizon server can share the file.

/// One value of the tree.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Null,
    /// A plain scalar, kept as its text; readers convert.
    Scalar(String),
    /// A string that must read back as a string (`'3'`): written quoted when it looks like a
    /// number or a bool, as SnakeYAML writes a `String` field.
    Str(String),
    List(Vec<Node>),
    /// Keys in file order (Citizens writes them in field order).
    Map(Vec<(String, Node)>),
}

impl Default for Node {
    fn default() -> Self {
        Node::Map(Vec::new())
    }
}

impl Node {
    pub fn map() -> Self {
        Node::Map(Vec::new())
    }
    pub fn scalar(v: impl ToString) -> Self {
        Node::Scalar(v.to_string())
    }
    /// A `double` as Java's `Double.toString` writes it (what Bukkit saves): `20.0`, `1.0`,
    /// `0.25`, `1.0E-4`. `Node::scalar(20.0)` would write `20`, which SnakeYAML loads as an
    /// `Integer` (review S1: `dropChances` / `weapon_damage_map` then throw on Paper).
    pub fn double(v: f64) -> Self {
        Node::Scalar(java_double(v))
    }
    /// A `float` (yaw, pitch) as Java's `Float.toString` writes it.
    pub fn float(v: f32) -> Self {
        Node::Scalar(java_float(v))
    }
    pub fn string(v: impl Into<String>) -> Self {
        Node::Str(v.into())
    }
    /// A list of strings, as an `ArrayList<String>` field is saved.
    pub fn string_list(items: &[String]) -> Self {
        Node::List(items.iter().map(|s| Node::Str(s.clone())).collect())
    }
    /// The child `key` of a map, or a dotted path (`allIgnores.targets`).
    pub fn get(&self, path: &str) -> Option<&Node> {
        let mut at = self;
        for part in path.split('.') {
            match at {
                Node::Map(entries) => at = &entries.iter().find(|(k, _)| k == part)?.1,
                _ => return None,
            }
        }
        Some(at)
    }
    pub fn str(&self, path: &str) -> Option<&str> {
        match self.get(path)? {
            Node::Scalar(s) | Node::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn f64(&self, path: &str) -> Option<f64> {
        self.str(path)?.trim().parse().ok()
    }
    pub fn i64(&self, path: &str) -> Option<i64> {
        let s = self.str(path)?.trim();
        s.parse::<i64>().ok().or_else(|| s.parse::<f64>().ok().map(|f| f as i64))
    }
    pub fn bool(&self, path: &str) -> Option<bool> {
        match self.str(path)?.trim() {
            "true" | "True" | "TRUE" | "yes" | "on" => Some(true),
            "false" | "False" | "FALSE" | "no" | "off" => Some(false),
            _ => None,
        }
    }
    /// A list of scalars (an `ArrayList<String>` field); a missing key is empty.
    pub fn strings(&self, path: &str) -> Vec<String> {
        match self.get(path) {
            Some(Node::List(items)) => items
                .iter()
                .filter_map(|n| match n {
                    Node::Scalar(s) | Node::Str(s) => Some(s.clone()),
                    _ => None,
                })
                .collect(),
            // Citizens' old layout: a map whose values are the entries (`updateOld`).
            Some(Node::Map(entries)) => entries
                .iter()
                .filter_map(|(_, n)| match n {
                    Node::Scalar(s) | Node::Str(s) => Some(s.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }
    /// Sets `key` on a map node (replacing an existing one in place).
    pub fn set(&mut self, key: &str, value: Node) {
        if !matches!(self, Node::Map(_)) {
            *self = Node::map();
        }
        if let Node::Map(entries) = self {
            if let Some(e) = entries.iter_mut().find(|(k, _)| k == key) {
                e.1 = value;
            } else {
                entries.push((key.to_owned(), value));
            }
        }
    }
    pub fn remove(&mut self, key: &str) {
        if let Node::Map(entries) = self {
            entries.retain(|(k, _)| k != key);
        }
    }
    pub fn is_empty(&self) -> bool {
        match self {
            Node::Null => true,
            Node::Scalar(s) | Node::Str(s) => s.is_empty(),
            Node::List(l) => l.is_empty(),
            Node::Map(m) => m.is_empty(),
        }
    }
}

/// Error from the YAML reader: the 1-based line and what was wrong.
#[derive(Debug, Clone, PartialEq)]
pub struct YamlError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for YamlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

struct Line<'a> {
    no: usize,
    indent: usize,
    text: &'a str,
}

/// Reads block-style YAML as SnakeYAML writes it: maps, lists (also at their key's indent),
/// plain, single- and double-quoted scalars, `[]` / `{}` for empty collections. Anchors,
/// multi-line scalars and flow collections with content are refused.
pub fn parse_yaml(text: &str) -> Result<Node, YamlError> {
    let lines: Vec<Line> = text
        .lines()
        .enumerate()
        .filter_map(|(i, raw)| {
            let trimmed = raw.trim_end();
            let content = trimmed.trim_start();
            if content.is_empty() || content.starts_with('#') || content == "---" {
                return None;
            }
            Some(Line { no: i + 1, indent: trimmed.len() - content.len(), text: content })
        })
        .collect();
    let mut pos = 0;
    if lines.is_empty() {
        return Ok(Node::map());
    }
    let indent = lines[0].indent;
    let node = parse_block(&lines, &mut pos, indent)?;
    if pos < lines.len() {
        return Err(YamlError { line: lines[pos].no, message: "unexpected indentation".into() });
    }
    Ok(node)
}

fn parse_block(lines: &[Line], pos: &mut usize, indent: usize) -> Result<Node, YamlError> {
    if lines[*pos].text.starts_with("- ") || lines[*pos].text == "-" {
        parse_list(lines, pos, indent)
    } else {
        parse_map(lines, pos, indent)
    }
}

fn parse_list(lines: &[Line], pos: &mut usize, indent: usize) -> Result<Node, YamlError> {
    let mut items = Vec::new();
    while *pos < lines.len() && lines[*pos].indent == indent && is_item(lines[*pos].text) {
        let line = &lines[*pos];
        let rest = line.text[1..].trim_start();
        *pos += 1;
        if rest.is_empty() {
            if *pos < lines.len() && lines[*pos].indent > indent {
                let child = lines[*pos].indent;
                items.push(parse_block(lines, pos, child)?);
            } else {
                items.push(Node::Null);
            }
        } else if let Some((key, value)) = split_key(rest) {
            // `- key: value` starts a map whose further keys sit two columns in.
            let inner = indent + (line.text.len() - rest.len());
            let mut entries = vec![(key, inline_or_block(lines, pos, inner, value, line.no)?)];
            if *pos < lines.len() && lines[*pos].indent == inner && !is_item(lines[*pos].text) {
                if let Node::Map(more) = parse_map(lines, pos, inner)? {
                    entries.extend(more);
                }
            }
            items.push(Node::Map(entries));
        } else {
            items.push(scalar(rest, line.no)?);
        }
    }
    Ok(Node::List(items))
}

fn is_item(text: &str) -> bool {
    text == "-" || text.starts_with("- ")
}

fn parse_map(lines: &[Line], pos: &mut usize, indent: usize) -> Result<Node, YamlError> {
    let mut entries: Vec<(String, Node)> = Vec::new();
    while *pos < lines.len() && lines[*pos].indent == indent && !is_item(lines[*pos].text) {
        let line = &lines[*pos];
        let Some((key, value)) = split_key(line.text) else {
            return Err(YamlError { line: line.no, message: format!("expected `key: value`, got `{}`", line.text) });
        };
        *pos += 1;
        let node = inline_or_block(lines, pos, indent, value, line.no)?;
        entries.push((key, node));
    }
    Ok(Node::Map(entries))
}

/// The value after `key:`: inline, or the block on the next lines (deeper, or a list at the
/// same indent, as Bukkit writes lists).
fn inline_or_block(
    lines: &[Line],
    pos: &mut usize,
    indent: usize,
    value: &str,
    no: usize,
) -> Result<Node, YamlError> {
    if !value.is_empty() {
        return scalar(value, no);
    }
    if *pos < lines.len() {
        let next = &lines[*pos];
        if next.indent > indent {
            return parse_block(lines, pos, next.indent);
        }
        if next.indent == indent && is_item(next.text) {
            return parse_list(lines, pos, indent);
        }
    }
    Ok(Node::Null)
}

/// Splits `key: value` (the key may be quoted); None when the line is no mapping.
fn split_key(text: &str) -> Option<(String, &str)> {
    if let Some(q) = text.chars().next().filter(|c| *c == '\'' || *c == '"') {
        let end = text[1..].find(q)? + 1;
        let rest = text[end + 1..].strip_prefix(':')?;
        if !(rest.is_empty() || rest.starts_with(' ')) {
            return None;
        }
        return Some((text[1..end].to_owned(), rest.trim()));
    }
    let mut search = 0;
    while let Some(i) = text[search..].find(':') {
        let at = search + i;
        let after = &text[at + 1..];
        if after.is_empty() || after.starts_with(' ') {
            return Some((text[..at].trim().to_owned(), after.trim()));
        }
        search = at + 1;
    }
    None
}

fn scalar(text: &str, no: usize) -> Result<Node, YamlError> {
    let err = |m: &str| YamlError { line: no, message: m.to_owned() };
    match text {
        "[]" => return Ok(Node::List(Vec::new())),
        "{}" => return Ok(Node::map()),
        "~" | "null" => return Ok(Node::Null),
        _ => {}
    }
    if let Some(inner) = text.strip_prefix('\'') {
        let inner = inner.strip_suffix('\'').ok_or_else(|| err("unterminated quote"))?;
        return Ok(Node::Str(inner.replace("''", "'")));
    }
    if let Some(inner) = text.strip_prefix('"') {
        let inner = inner.strip_suffix('"').ok_or_else(|| err("unterminated quote"))?;
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    let code = u32::from_str_radix(&hex, 16).map_err(|_| err("bad \\u escape"))?;
                    out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                }
                Some(other) => out.push(other),
                None => return Err(err("dangling escape")),
            }
        }
        return Ok(Node::Str(out));
    }
    if text.starts_with('&') || text.starts_with('*') || text.starts_with('|') || text.starts_with('>') {
        return Err(err("anchors and multi-line scalars are not supported"));
    }
    if (text.starts_with('[') || text.starts_with('{')) && text.len() > 2 {
        return Err(err("flow collections with content are not supported"));
    }
    // A trailing comment after a plain scalar.
    let plain = match text.find(" #") {
        Some(i) => text[..i].trim_end(),
        None => text,
    };
    Ok(Node::Scalar(plain.to_owned()))
}

/// Writes the tree as SnakeYAML's block style with `indent` leading spaces (lists at their
/// key's indent, as Bukkit does).
pub fn to_yaml(node: &Node, indent: usize) -> String {
    let mut out = String::new();
    write_node(&mut out, node, indent);
    out
}

fn write_node(out: &mut String, node: &Node, indent: usize) {
    let pad = " ".repeat(indent);
    match node {
        Node::Map(entries) => {
            for (k, v) in entries {
                out.push_str(&pad);
                out.push_str(&quote_key(k));
                out.push(':');
                match v {
                    Node::Map(m) if !m.is_empty() => {
                        out.push('\n');
                        write_node(out, v, indent + 2);
                    }
                    Node::List(l) if !l.is_empty() => {
                        out.push('\n');
                        write_node(out, v, indent);
                    }
                    other => {
                        out.push(' ');
                        out.push_str(&inline(other));
                        out.push('\n');
                    }
                }
            }
        }
        Node::List(items) => {
            for item in items {
                out.push_str(&pad);
                out.push('-');
                match item {
                    Node::Map(m) if !m.is_empty() => {
                        // First key on the dash line, the rest two columns in.
                        let mut nested = String::new();
                        write_node(&mut nested, item, indent + 2);
                        out.push(' ');
                        out.push_str(nested.trim_start());
                    }
                    Node::List(l) if !l.is_empty() => {
                        out.push('\n');
                        write_node(out, item, indent + 2);
                    }
                    other => {
                        out.push(' ');
                        out.push_str(&inline(other));
                        out.push('\n');
                    }
                }
            }
        }
        other => {
            out.push_str(&pad);
            out.push_str(&inline(other));
            out.push('\n');
        }
    }
}

fn inline(node: &Node) -> String {
    match node {
        Node::Null => "null".into(),
        Node::Scalar(s) => quote_value(s),
        Node::Str(s) if s.parse::<f64>().is_ok() => format!("'{s}'"),
        Node::Str(s) => quote_value(s),
        Node::List(_) => "[]".into(),
        Node::Map(_) => "{}".into(),
    }
}

fn quote_key(k: &str) -> String {
    if k.is_empty() || needs_quotes(k) || k.parse::<f64>().is_ok() {
        format!("'{}'", k.replace('\'', "''"))
    } else {
        k.to_owned()
    }
}

/// SnakeYAML quotes a string that would read back as something else (a number, a bool) or that
/// holds YAML syntax; numbers written as numbers stay plain.
fn quote_value(s: &str) -> String {
    if s.is_empty() {
        return "''".into();
    }
    if needs_quotes(s) {
        return format!("'{}'", s.replace('\'', "''"));
    }
    s.to_owned()
}

fn needs_quotes(s: &str) -> bool {
    let first = s.chars().next().unwrap_or(' ');
    s.starts_with(' ')
        || s.ends_with(' ')
        || "-?:,[]{}#&*!|>'\"%@`".contains(first)
        || s.contains(": ")
        || s.contains(" #")
        || matches!(s, "true" | "false" | "yes" | "no" | "on" | "off" | "null" | "~")
}

#[cfg(test)]
mod tests {
    #[test]
    fn doubles_as_java_writes_them() {
        // Review S1.
        assert_eq!(super::java_double(20.0), "20.0");
        assert_eq!(super::java_double(1.0), "1.0");
        assert_eq!(super::java_double(0.25), "0.25");
        assert_eq!(super::java_double(-30.5), "-30.5");
        assert_eq!(super::java_double(0.0), "0.0");
        assert_eq!(super::java_double(1e-4), "1.0E-4");
        assert_eq!(super::java_double(12345678.0), "1.2345678E7");
        assert_eq!(super::java_float(90.0), "90.0");
        assert_eq!(super::java_float(0.1), "0.1");
        assert_eq!(super::Node::double(7.0), super::Node::Scalar("7.0".into()));
    }

    use super::*;

    const SAMPLE: &str = "\
stats_punches: 12
respawnTime: 0
allIgnores:
  byNpcName:
  - '3'
  - '16'
  targets:
  - OWNER
allTargets:
  targets:
  - NPC
avoidReturnPoint:
  world: world
  x: 78.5
  yaw: 0.0
weapon_damage_map: {}
drops:
- ==: org.bukkit.inventory.ItemStack
  v: 4189
  type: BREAD
";

    #[test]
    fn reads_the_citizens_layout() {
        let n = parse_yaml(SAMPLE).unwrap();
        assert_eq!(n.i64("stats_punches"), Some(12));
        assert_eq!(n.strings("allIgnores.byNpcName"), vec!["3", "16"]);
        assert_eq!(n.strings("allIgnores.targets"), vec!["OWNER"]);
        assert_eq!(n.strings("allTargets.targets"), vec!["NPC"]);
        assert_eq!(n.f64("avoidReturnPoint.x"), Some(78.5));
        assert_eq!(n.get("weapon_damage_map"), Some(&Node::map()));
        match n.get("drops") {
            Some(Node::List(items)) => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].str("type"), Some("BREAD"));
                assert_eq!(items[0].str("=="), Some("org.bukkit.inventory.ItemStack"));
            }
            other => panic!("drops: {other:?}"),
        }
    }

    #[test]
    fn round_trips() {
        let n = parse_yaml(SAMPLE).unwrap();
        let again = parse_yaml(&to_yaml(&n, 0)).unwrap();
        assert_eq!(n, again);
    }

    #[test]
    fn quoted_scalars() {
        let n = parse_yaml("a: 'it''s'\nb: \"x\\ny\"\n'3': v\n").unwrap();
        assert_eq!(n.str("a"), Some("it's"));
        assert_eq!(n.str("b"), Some("x\ny"));
        assert_eq!(n.str("3"), Some("v"));
    }
}

/// Java's `Double.toString`: plain with at least one decimal for 1e-3 <= |v| < 1e7, else
/// computerised scientific notation (`1.0E-4`, `1.2345678E7`).
pub fn java_double(v: f64) -> String {
    java_number(v, format!("{v}"), format!("{v:e}"))
}

/// Java's `Float.toString` (shortest digits of the `f32`, same layout as `java_double`).
pub fn java_float(v: f32) -> String {
    java_number(f64::from(v), format!("{v}"), format!("{v:e}"))
}

fn java_number(v: f64, plain: String, sci: String) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    let a = v.abs();
    if v == 0.0 || (1e-3..1e7).contains(&a) {
        let mut s = if v == 0.0 && v.is_sign_negative() { "-0".to_owned() } else if v == 0.0 { "0".to_owned() } else { plain };
        if !s.contains('.') {
            s.push_str(".0");
        }
        return s;
    }
    // Rust: `1e-4`, `1.2345678e7`; Java: `1.0E-4`, `1.2345678E7`.
    let (mant, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let mant = if mant.contains('.') { mant.to_owned() } else { format!("{mant}.0") };
    format!("{mant}E{exp}")
}
