use crate::ver::Ver;
use crate::Result;

const GROUPS: [(&str, &str); 7] = [
    ("aoEx", "ao"),
    ("naEx", "na"),
    ("freeEx", "free"),
    ("expEx", "exp"),
    ("premiumExpEx", "premium"),
    ("podEx", "pod"),
    ("updatesEx", "updates"),
];

pub struct Extracted {
    pub patches: String,
    pub free_css: String,
    pub podcasts_css: String,
    pub client: String,
}

pub fn extract(script: &str, floor: Ver) -> Result<Extracted> {
    let mut patches = String::new();
    for (array, group) in GROUPS {
        let records = array_items(script, array).ok_or(format!("spotx.sh has no {array} array"))?;
        for rec in records {
            let max = rec.split('&').nth(6).and_then(Ver::parse);
            if max.is_none_or(|m| m >= floor) {
                patches += &format!("{group}&{rec}\n");
            }
        }
    }
    let var = |name: &str| assignment(script, name).ok_or(format!("spotx.sh has no {name}"));
    let line = |name: &str| var(name).map(|v| v.trim_start().to_string());
    Ok(Extracted {
        patches,
        free_css: [
            line("hideDLIcon")?,
            line("hideDLMenu")?,
            line("hideDLQual")?,
            line("hideVeryHigh")?,
        ]
        .join("\n")
            + "\n",
        podcasts_css: line("hideSubfeed")? + "\n",
        client: var("buildVer")?,
    })
}

fn array_items(script: &str, name: &str) -> Option<Vec<String>> {
    let start = script.find(&format!("\n{name}=(\n"))? + name.len() + 3;
    let body = &script[start..start + script[start..].find("\n)")?];
    Some(
        body.lines()
            .map(str::trim)
            .filter(|l| l.starts_with('\''))
            .map(unquote)
            .collect(),
    )
}

fn assignment(script: &str, name: &str) -> Option<String> {
    let line = script
        .lines()
        .find(|l| l.starts_with(&format!("{name}=")))?;
    let value = &line[name.len() + 1..];
    match value.chars().next()? {
        '\'' => Some(unquote(value)),
        '"' => Some(value.trim_matches('"').to_string()),
        _ => None,
    }
}

fn unquote(word: &str) -> String {
    let (mut out, mut quoted, mut chars) = (String::new(), false, word.chars());
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (_, '\'') => quoted = !quoted,
            (false, '\\') => out.extend(chars.next()),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = "buildVer=\"1.3.6.1921.gabc\"\nhideDLIcon=' .a {display:none}'\nhideDLMenu=' .b'\nhideDLQual=' .c'\nhideVeryHigh=' .d'\nhideSubfeed=' .e'\n\
aoEx=(\n'x&a\\Kb&c&s&xpuiJs&1.2.0.0'\n#'off&z&z&&xpuiJs'\n'old&y&y&&xpuiJs&1.1.0.0&1.2.0.0'\n'q&it'\\''s&ok&&xpuiJs'\n)\n\
naEx=(\n)\nfreeEx=(\n)\nexpEx=(\n)\npremiumExpEx=(\n)\npodEx=(\n)\nupdatesEx=(\n)\n";

    #[test]
    fn parses_records_and_css() {
        let e = extract(SCRIPT, Ver([1, 3, 0, 0])).unwrap();
        assert_eq!(
            e.patches,
            "ao&x&a\\Kb&c&s&xpuiJs&1.2.0.0\nao&q&it's&ok&&xpuiJs\n"
        );
        assert_eq!(e.free_css, ".a {display:none}\n.b\n.c\n.d\n");
        assert_eq!(e.podcasts_css, ".e\n");
        assert_eq!(e.client, "1.3.6.1921.gabc");
    }
}
