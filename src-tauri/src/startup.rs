use std::{collections::HashSet, path::Path, process::Command};

use serde::Serialize;

use crate::{models::DocumentStatus, observer};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartProjectResult {
    pub scripts_started: usize,
    pub websites_opened: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct StartupPlan {
    pub scripts: Vec<String>,
    pub urls: Vec<String>,
}

pub fn load_plan(root: &Path) -> Result<StartupPlan, String> {
    let document = observer::observe_documents(root).startup;
    match document.status {
        DocumentStatus::Missing => return Err("STARTUP.md was not found".to_string()),
        DocumentStatus::Error => {
            return Err(document
                .error
                .unwrap_or_else(|| "STARTUP.md could not be read".to_string()));
        }
        DocumentStatus::Available => {}
    }
    if document.truncated {
        return Err("STARTUP.md is larger than 2 MB and cannot be run safely".to_string());
    }

    let plan = parse_startup(document.content.as_deref().unwrap_or_default());
    if plan.scripts.is_empty() && plan.urls.is_empty() {
        return Err(
            "STARTUP.md has no fenced PowerShell scripts or HTTP(S) website links".to_string(),
        );
    }
    Ok(plan)
}

pub fn start_scripts(root: &Path, scripts: &[String]) -> Result<(), String> {
    for (index, script) in scripts.iter().enumerate() {
        let mut command = Command::new("powershell.exe");
        command
            .args(["-NoLogo", "-NoExit", "-Command", script])
            .current_dir(root);

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
            command.creation_flags(CREATE_NEW_CONSOLE);
        }

        command.spawn().map_err(|error| {
            format!(
                "Unable to start PowerShell script {} in a new console: {error}",
                index + 1
            )
        })?;
    }
    Ok(())
}

fn parse_startup(markdown: &str) -> StartupPlan {
    let mut scripts = Vec::new();
    let mut prose = String::new();
    let mut fence: Option<(char, usize, bool)> = None;
    let mut script_lines = Vec::new();

    for line in markdown.lines() {
        if let Some((marker, width, is_powershell)) = fence {
            if is_closing_fence(line, marker, width) {
                if is_powershell {
                    let script = script_lines.join("\n");
                    if !script.trim().is_empty() {
                        scripts.push(script);
                    }
                }
                script_lines.clear();
                fence = None;
            } else if is_powershell {
                script_lines.push(line);
            }
            continue;
        }

        if let Some((marker, width, info)) = opening_fence(line) {
            let language = info.split_whitespace().next().unwrap_or_default();
            fence = Some((marker, width, language.eq_ignore_ascii_case("powershell")));
            continue;
        }

        prose.push_str(line);
        prose.push('\n');
    }

    if matches!(fence, Some((_, _, true))) {
        let script = script_lines.join("\n");
        if !script.trim().is_empty() {
            scripts.push(script);
        }
    }

    StartupPlan {
        scripts,
        urls: extract_http_urls(&prose),
    }
}

fn opening_fence(line: &str) -> Option<(char, usize, &str)> {
    let trimmed = line.trim_start();
    let marker = trimmed.chars().next()?;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let width = trimmed.chars().take_while(|value| *value == marker).count();
    (width >= 3).then(|| (marker, width, trimmed[width..].trim()))
}

fn is_closing_fence(line: &str, marker: char, width: usize) -> bool {
    let trimmed = line.trim();
    let marker_count = trimmed.chars().take_while(|value| *value == marker).count();
    marker_count >= width && trimmed.chars().all(|value| value == marker)
}

fn extract_http_urls(markdown: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut seen = HashSet::new();
    let mut remaining = markdown;

    while let Some(start) = next_url_start(remaining) {
        let candidate = &remaining[start..];
        let end = candidate
            .char_indices()
            .find_map(|(index, value)| {
                (index > 0
                    && (value.is_whitespace() || matches!(value, '<' | '>' | '"' | '\'' | ']')))
                .then_some(index)
            })
            .unwrap_or(candidate.len());
        let raw = &candidate[..end];
        let url = trim_url_ending(raw);
        if !url.is_empty() && seen.insert(url.to_string()) {
            urls.push(url.to_string());
        }
        remaining = &candidate[raw.len()..];
    }

    urls
}

fn next_url_start(value: &str) -> Option<usize> {
    [value.find("https://"), value.find("http://")]
        .into_iter()
        .flatten()
        .min()
}

fn trim_url_ending(value: &str) -> &str {
    let mut trimmed = value.trim_end_matches(['.', ',', ';', ':', '!', '?']);
    while trimmed.ends_with(')') && trimmed.matches(')').count() > trimmed.matches('(').count() {
        trimmed = &trimmed[..trimmed.len() - 1];
    }
    trimmed
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{load_plan, parse_startup};

    #[test]
    fn parses_each_powershell_fence_as_an_independent_script() {
        let plan = parse_startup(
            "# Start\n\n```powershell\nnpm run backend\n```\n\n~~~PowerShell\nnpm run frontend\n~~~\n",
        );

        assert_eq!(
            plan.scripts,
            vec![
                "npm run backend".to_string(),
                "npm run frontend".to_string()
            ]
        );
    }

    #[test]
    fn ignores_other_code_fences_and_urls_inside_scripts() {
        let plan = parse_startup(
            "```bash\nnpm start\n```\n```powershell\nnpm run dev -- --url http://internal.test\n```\n",
        );

        assert_eq!(
            plan.scripts,
            vec!["npm run dev -- --url http://internal.test".to_string()]
        );
        assert!(plan.urls.is_empty());
    }

    #[test]
    fn extracts_and_deduplicates_http_websites_from_startup_prose() {
        let plan = parse_startup(
            "Open [frontend](http://127.0.0.1:3000), https://example.com/path?q=1, and <http://127.0.0.1:3000>.\n",
        );

        assert_eq!(
            plan.urls,
            vec![
                "http://127.0.0.1:3000".to_string(),
                "https://example.com/path?q=1".to_string(),
            ]
        );
    }

    #[test]
    fn loads_the_discovered_startup_document_from_a_registered_root() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("docs")).unwrap();
        fs::write(
            temp.path().join("docs").join("startup.md"),
            "```powershell\nnpm run dev\n```\n\n[App](http://127.0.0.1:3000)\n",
        )
        .unwrap();

        let plan = load_plan(temp.path()).unwrap();

        assert_eq!(plan.scripts, vec!["npm run dev".to_string()]);
        assert_eq!(plan.urls, vec!["http://127.0.0.1:3000".to_string()]);
    }

    #[test]
    fn rejects_startup_documents_without_runnable_content() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("STARTUP.md"), "# Setup only\n").unwrap();

        assert_eq!(
            load_plan(temp.path()).unwrap_err(),
            "STARTUP.md has no fenced PowerShell scripts or HTTP(S) website links"
        );
    }
}
