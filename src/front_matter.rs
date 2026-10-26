/// Reads front matter as keys and values, in the order written. Only flat keys are read: a plain,
/// quoted or `[a, b]` value, or a list of `- item` lines under an empty key, each list joined
/// with commas. Anything else, such as a nested map or a block scalar, gives `None`.
pub fn parse(yaml: &str) -> Option<Vec<(String, String)>> {
    let mut pairs: Vec<(String, Vec<String>)> = Vec::new();
    for line in yaml.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if line.starts_with([' ', '\t']) {
            let item = trimmed.strip_prefix("- ")?;
            let (_, items) = pairs.last_mut()?;
            items.push(unquote(item.trim()).to_string());
            continue;
        }
        let (key, value) = match line.split_once(": ") {
            Some(pair) => pair,
            None => (line.strip_suffix(':')?, ""),
        };
        if key.is_empty()
            || !key
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            return None;
        }
        let value = value.trim();
        let items = if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            inner
                .split(',')
                .map(|item| unquote(item.trim()).to_string())
                .filter(|item| !item.is_empty())
                .collect()
        } else if value.starts_with(['|', '>', '{', '&', '*', '!']) {
            return None;
        } else if value.is_empty() {
            Vec::new()
        } else {
            vec![unquote(value).to_string()]
        };
        pairs.push((key.to_string(), items));
    }
    Some(
        pairs
            .into_iter()
            .map(|(key, items)| (key, items.join(", ")))
            .collect(),
    )
}

fn unquote(value: &str) -> &str {
    ['"', '\'']
        .iter()
        .find_map(|&quote| value.strip_prefix(quote)?.strip_suffix(quote))
        .unwrap_or(value)
}
