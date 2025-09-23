// Path utilities

pub fn join(base: &str, path: &str) -> String {
    if path.starts_with('/') {
        return path.to_string();
    }
    if base == "/" {
        format!("/{}", path)
    } else if base.is_empty() {
        path.to_string()
    } else {
        format!("{}/{}", base, path)
    }
}

pub fn dirname(path: &str) -> &str {
    if path == "/" || path.is_empty() {
        return "";
    }
    if let Some(pos) = path.rfind('/') {
        if pos == 0 { "" } else { &path[..pos] }
    } else {
        ""
    }
}

pub fn basename(path: &str) -> &str {
    if let Some(pos) = path.rfind('/') {
        &path[pos + 1..]
    } else {
        path
    }
}

pub fn is_ancestor_or_equal(ancestor: &str, descendant: &str) -> bool {
    if ancestor == descendant {
        return true;
    }
    if ancestor == "/" {
        return true;
    }
    if descendant == "/" || descendant.is_empty() {
        return false;
    }
    descendant.starts_with(ancestor) && descendant.chars().nth(ancestor.len()) == Some('/')
}

pub fn lca(a: &str, b: &str) -> String {
    if a == b {
        return dirname(a).to_string();
    }
    if a.is_empty() {
        return b.to_string();
    }
    if b.is_empty() {
        return a.to_string();
    }
    if dirname(a) == dirname(b) {
        return dirname(a).to_string();
    }
    if is_ancestor_or_equal(a, b) {
        return a.to_string();
    }
    if is_ancestor_or_equal(b, a) {
        return b.to_string();
    }
    lca(dirname(a), dirname(b))
}

// Simple exact string matching (no wildcards)
pub fn event_matches(pattern: &str, text: &str) -> bool {
    pattern == text
}
