//! Minimal Valve Data Format (VDF) text helpers for Steam manifests.
//!
//! Only extracts quoted `"key" "value"` pairs and `"path"` library entries.
//! No binary VDF; no process scrape.

/// Return the value for the first `"key" "value"` line matching `key`.
pub fn vdf_quoted_value(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        let mut parts = trimmed.split('"').filter(|p| !p.trim().is_empty());
        let found = parts.next()?;
        if found != key {
            continue;
        }
        return parts.next().map(|s| s.to_string());
    }
    None
}

/// Collect every `"path"` value from a libraryfolders.vdf body.
pub fn vdf_all_paths(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let mut parts = trimmed.split('"').filter(|p| !p.trim().is_empty());
        let Some(key) = parts.next() else {
            continue;
        };
        if key != "path" {
            continue;
        }
        if let Some(value) = parts.next() {
            let normalized = value.replace("\\\\", "\\");
            if !normalized.is_empty() {
                out.push(normalized);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_quoted_value() {
        let text = "\"appid\"\t\t\"480\"\n\"name\"\t\t\"Spacewar\"\n";
        assert_eq!(vdf_quoted_value(text, "appid").as_deref(), Some("480"));
        assert_eq!(vdf_quoted_value(text, "name").as_deref(), Some("Spacewar"));
    }

    #[test]
    fn extracts_library_paths() {
        let text = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Steam"
	}
	"1"
	{
		"path"		"D:\\Games\\SteamLibrary"
	}
}
"#;
        let paths = vdf_all_paths(text);
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], "C:\\Steam");
        assert_eq!(paths[1], "D:\\Games\\SteamLibrary");
    }
}
