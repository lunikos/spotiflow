use fancy_regex::{Regex, RegexBuilder};
use regex_automata::{meta, Input};

use crate::Result;

pub struct Compiled {
    regex: Regex,
    replace: String,
    all: bool,
    prefilter: Option<(Finder, Regex)>,
    window: Option<usize>,
}

enum Finder {
    Literal(String),
    Relaxed(meta::Regex),
}

impl Finder {
    fn next(&self, text: &str, pos: usize) -> Option<usize> {
        match self {
            Finder::Literal(lit) => text[pos..].find(lit.as_str()).map(|i| i + pos),
            Finder::Relaxed(re) => re.find(Input::new(text).range(pos..)).map(|m| m.start()),
        }
    }
}

impl Compiled {
    pub fn replace(&self, text: &str) -> Result<Option<String>> {
        let Some((finder, anchored)) = &self.prefilter else {
            if !self.regex.is_match(text)? {
                return Ok(None);
            }
            return Ok(Some(
                self.regex
                    .try_replacen(text, if self.all { 0 } else { 1 }, self.replace.as_str())?
                    .into_owned(),
            ));
        };
        let (mut out, mut last, mut pos, mut hit) = (String::new(), 0, 0, false);
        while let Some(at) = finder.next(text, pos) {
            let step = at + text[at..].chars().next().map_or(1, char::len_utf8);
            let mut end = self.window.map_or(text.len(), |w| (at + w).min(text.len()));
            while !text.is_char_boundary(end) {
                end += 1;
            }
            let Some(caps) = anchored.captures(&text[at..end])? else {
                pos = step;
                continue;
            };
            hit = true;
            out.push_str(&text[last..at]);
            caps.expand(&self.replace, &mut out);
            last = at + caps.get(0).map_or(0, |m| m.end());
            if !self.all {
                break;
            }
            pos = last.max(step);
        }
        if !hit {
            return Ok(None);
        }
        out.push_str(&text[last..]);
        Ok(Some(out))
    }
}

pub fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

pub fn unlatin1(s: &str) -> Result<Vec<u8>> {
    s.chars()
        .map(|c| {
            u8::try_from(u32::from(c))
                .map_err(|_| format!("patch produced non-byte character {c:?}").into())
        })
        .collect()
}

pub fn compile(regex: &str, replace: &str, flags: &str) -> Result<Vec<Compiled>> {
    let regex = latin1(regex.as_bytes());
    let replace = latin1(replace.as_bytes());
    let replace = if flags.contains('e') {
        eval_concat(&replace)?
    } else {
        replace
    };
    let all = flags.contains('g');
    let prefix = if flags.contains('s') { "(?s)" } else { "" };

    alternatives(&regex)
        .into_iter()
        .map(|alt| {
            let (pattern, shift) = match alt.split_once("\\K") {
                Some((pre, post)) => (format!("({pre}){post}"), 1),
                None => (alt.clone(), 0),
            };
            let body = perl_classes(&pattern, shift);
            let build = |p: String| -> Result<Regex> {
                Ok(RegexBuilder::new(&p).backtrack_limit(usize::MAX).build()?)
            };
            let regex = build(format!("{prefix}{body}"))?;
            let needs_context = ["(?<=", "(?<!", "\\b", "\\B"]
                .iter()
                .any(|t| body.contains(t));
            let finder = match leading_literal(&alt) {
                _ if needs_context => None,
                Some(lit) => Some(Finder::Literal(lit)),
                None => without_lookarounds(&body)
                    .and_then(|r| meta::Regex::new(&format!("{prefix}{r}")).ok())
                    .map(Finder::Relaxed),
            };
            let prefilter = match finder {
                Some(f) => Some((f, build(format!("{prefix}^(?:{body})"))?)),
                None => None,
            };
            let window = max_len(&format!("{prefix}{body}"));
            let mut rep = String::new();
            if shift == 1 {
                rep.push_str("${1}");
            }
            rep.push_str(&replacement(&replace, shift));
            Ok(Compiled {
                regex,
                replace: rep,
                all,
                prefilter,
                window,
            })
        })
        .collect()
}

fn max_len(re: &str) -> Option<usize> {
    let consumed = re.replace("(?=", "(?:").replace("(?!", "(?:");
    regex_syntax::parse(&consumed)
        .ok()?
        .properties()
        .maximum_len()
}

fn without_lookarounds(re: &str) -> Option<String> {
    let (mut out, mut skip_depth, mut depth, mut esc, mut class) =
        (String::new(), None, 0usize, false, false);
    let mut chars = re.chars().peekable();
    while let Some(c) = chars.next() {
        let start_len = out.len();
        out.push(c);
        if esc {
            esc = false;
            if c.is_ascii_digit() && c != '0' && !class {
                return None;
            }
        } else if c == '\\' {
            esc = true;
        } else if class {
            class = c != ']';
        } else if c == '[' {
            class = true;
            if let Some(c) = chars.next_if_eq(&']') {
                out.push(c);
            }
        } else if c == '(' {
            depth += 1;
            let rest: String = chars.clone().take(3).collect();
            if skip_depth.is_none()
                && ["?=", "?!", "?<=", "?<!"]
                    .iter()
                    .any(|p| rest.starts_with(p))
            {
                skip_depth = Some(depth);
                out.truncate(start_len);
            }
        } else if c == ')' {
            if skip_depth == Some(depth) {
                skip_depth = None;
                out.truncate(start_len);
                depth -= 1;
                continue;
            }
            depth = depth.saturating_sub(1);
        }
        if skip_depth.is_some() {
            out.truncate(start_len);
        }
    }
    Some(out)
}

fn leading_literal(re: &str) -> Option<String> {
    let (mut depth, mut esc) = (0i32, false);
    for c in re.chars() {
        match (esc, c) {
            (true, _) => esc = false,
            (_, '\\') => esc = true,
            (_, '(') => depth += 1,
            (_, ')') => depth -= 1,
            (_, '|') if depth == 0 => return None,
            _ => {}
        }
    }
    let (mut lit, mut chars) = (String::new(), re.chars().peekable());
    while let Some(c) = chars.next() {
        let ch = match c {
            '\\' => match chars.peek() {
                Some(&n) if n.is_ascii_punctuation() => {
                    chars.next();
                    n
                }
                _ => break,
            },
            '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' => break,
            _ => c,
        };
        if matches!(chars.peek(), Some('*' | '?' | '{')) {
            break;
        }
        lit.push(ch);
    }
    (lit.chars().count() >= 4).then_some(lit)
}

fn alternatives(re: &str) -> Vec<String> {
    match re.strip_prefix("(?|").and_then(|r| r.strip_suffix(')')) {
        Some(inner) => split_top(inner),
        None if re.contains("\\K") => split_top(re),
        None => vec![re.to_string()],
    }
}

fn split_top(re: &str) -> Vec<String> {
    let (mut depth, mut start, mut out, mut esc, mut class) = (0, 0, Vec::new(), false, false);
    for (i, c) in re.char_indices() {
        match (esc, class, c) {
            (true, _, _) => esc = false,
            (_, _, '\\') => esc = true,
            (_, true, ']') => class = false,
            (_, true, _) => {}
            (_, _, '[') => class = true,
            (_, _, '(') => depth += 1,
            (_, _, ')') => depth -= 1,
            (_, _, '|') if depth == 0 => {
                out.push(re[start..i].to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(re[start..].to_string());
    out
}

fn perl_classes(re: &str, shift: usize) -> String {
    let (mut out, mut chars, mut in_class) = (String::new(), re.chars().peekable(), false);
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let Some(n) = chars.next() else { break };
                let set = match n {
                    'w' => Some(("0-9A-Za-z_", false)),
                    'W' => Some(("0-9A-Za-z_", true)),
                    'd' => Some(("0-9", false)),
                    'D' => Some(("0-9", true)),
                    's' => Some(("\\t\\n\\x0B\\x0C\\r ", false)),
                    'S' => Some(("\\t\\n\\x0B\\x0C\\r ", true)),
                    _ => None,
                };
                if n.is_ascii_digit() && n != '0' && !in_class {
                    let mut num = n.to_string();
                    while let Some(d) = chars.peek().copied().filter(char::is_ascii_digit) {
                        num.push(d);
                        chars.next();
                    }
                    out.push_str(&format!("\\{}", num.parse::<usize>().unwrap_or(0) + shift));
                    continue;
                }
                match set {
                    Some((s, false)) if in_class => out.push_str(s),
                    Some((s, neg)) => out.push_str(&format!("[{}{s}]", if neg { "^" } else { "" })),
                    None if n.is_ascii_punctuation() && !"\\.+*?()|[]{}^$#&-~".contains(n) => {
                        out.push(n)
                    }
                    None => {
                        out.push('\\');
                        out.push(n);
                    }
                }
            }
            '[' if !in_class => {
                in_class = true;
                out.push(c);
                if let Some(c) = chars.next_if_eq(&'^') {
                    out.push(c);
                }
                if chars.peek() == Some(&']') {
                    out.push_str("\\]");
                    chars.next();
                }
            }
            ']' if in_class => {
                in_class = false;
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

fn replacement(rep: &str, shift: usize) -> String {
    let (mut out, mut chars) = (String::new(), rep.chars().peekable());
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('x') => {
                    let braced = chars.peek() == Some(&'{');
                    if braced {
                        chars.next();
                    }
                    let mut hex = String::new();
                    while let Some(&h) = chars.peek() {
                        if h.is_ascii_hexdigit() && (braced || hex.len() < 2) {
                            hex.push(h);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    if braced {
                        chars.next();
                    }
                    let ch =
                        char::from_u32(u32::from_str_radix(&hex, 16).unwrap_or(0)).unwrap_or('\0');
                    let lit = if (ch as u32) <= 0xFF {
                        ch.to_string()
                    } else {
                        latin1(ch.to_string().as_bytes())
                    };
                    push_literal(&mut out, &lit);
                }
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => push_literal(&mut out, &other.to_string()),
                None => {}
            },
            '$' => {
                let braced = chars.peek() == Some(&'{');
                if braced {
                    chars.next();
                }
                let mut num = String::new();
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() {
                        num.push(d);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if braced {
                    chars.next();
                }
                match num.parse::<usize>() {
                    Ok(n) => out.push_str(&format!("${{{}}}", n + shift)),
                    Err(_) => out.push_str("$$"),
                }
            }
            _ => push_literal(&mut out, &c.to_string()),
        }
    }
    out
}

fn push_literal(out: &mut String, s: &str) {
    out.push_str(&s.replace('$', "$$"));
}

fn eval_concat(code: &str) -> Result<String> {
    let mut out = String::new();
    for part in code.split(" . ") {
        let part = part.trim();
        if let Some(lit) = part.strip_prefix('"').and_then(|p| p.strip_suffix('"')) {
            out.push_str(&lit.replace('$', "\\$"));
        } else if part.starts_with('$') && part[1..].chars().all(|c| c.is_ascii_digit()) {
            out.push_str(part);
        } else {
            return Err(format!("unsupported Perl expression in replacement: {code}").into());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(re: &str, rep: &str, flags: &str, text: &str) -> String {
        let mut s = text.to_string();
        for c in compile(re, rep, flags).unwrap() {
            s = c.replace(&s).unwrap().unwrap_or(s);
        }
        s
    }

    #[test]
    fn keep_out_moves_into_a_group() {
        assert_eq!(run(r"(a)b\K(c)", "[$1$2]", "", "abc abc"), "ab[ac] abc");
        assert_eq!(run(r"x\Ky", "z", "g", "xy xy"), "xz xz");
        assert_eq!(
            run(r"(?:a:!|b:(\w+)=>\1\.c=!)\K0", "1", "", "b:q=>q.c=!0"),
            "b:q=>q.c=!1"
        );
    }

    #[test]
    fn perl_escapes_and_literals() {
        assert_eq!(
            run(r"\@webgate\/(gabo)", r#""@" . $1"#, "ge", "@webgate/gabo"),
            "@gabo"
        );
        assert_eq!(run(r"(a)", r"\x41$1\(\)", "", "a"), "Aa()");
        assert_eq!(unlatin1(&run("a", r"\xD5", "", "a")).unwrap(), vec![0xD5]);
        assert_eq!(run(r"\w+", "$", "", "é_x"), "é$");
        assert_eq!(run(r"(\d)1", "${1}2", "", "11"), "12");
    }

    #[test]
    fn keep_out_per_branch() {
        assert_eq!(
            run(
                r"/a\Kd(?=s/v1)|/a\Kd(?=s/v2)",
                "b",
                "gs",
                "/ads/v1 /ads/v2 /ads/v3"
            ),
            "/abs/v1 /abs/v2 /ads/v3"
        );
    }

    #[test]
    fn branch_reset_splits() {
        assert_eq!(
            run(
                r#"(?|(===")free(")|(")free("===))"#,
                "$1premium$2",
                "g",
                r#"a==="free" "free"===b"#
            ),
            r#"a==="premium" "premium"===b"#
        );
    }

    #[test]
    fn lookarounds_dropped() {
        assert_eq!(
            without_lookarounds(r"a(?=b(c))d(?!e)[(?=]").as_deref(),
            Some("ad[(?=]")
        );
        assert_eq!(without_lookarounds(r"(x)\1"), None);
        assert_eq!(
            run(
                r"[\x10-\x1F]\x01\K..(?=\xF0)",
                r"\x00\x00",
                "gs",
                "\x11\x01ab\u{f0}"
            ),
            "\x11\x01\0\0\u{f0}"
        );
    }

    #[test]
    fn whole_table_compiles() {
        for p in crate::patch::table() {
            compile(p.regex, p.replace, p.flags).unwrap_or_else(|e| panic!("{}: {e}", p.name));
        }
    }
}
