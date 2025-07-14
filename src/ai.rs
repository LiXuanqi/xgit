use std::process::Command;

/// Generate a commit message from a git diff using Claude AI
pub fn generate_commit_message(
    diff_text: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    if diff_text.is_empty() {
        return Ok(None);
    }

    // Prepare the prompt for Claude
    let prompt = format!(
        "Based on the following git diff, generate a conventional commit message.

The message should follow this format:
<type>[optional scope]: <description>

[optional body]

Choose type from: feat, fix, docs, style, refactor, test, chore
Keep the description under 50 characters, use imperative mood, and capitalize the first letter.

Respond with ONLY the commit message, no additional text or formatting.

Git diff:
{diff_text}"
    );

    // Call Claude CLI with JSON output
    let output = Command::new("claude")
        .arg("--print")
        .arg("--output-format")
        .arg("json")
        .arg(&prompt)
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let response = String::from_utf8_lossy(&output.stdout);

            // Parse Claude CLI JSON response and extract the result field
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&response)
                && let Some(message) = json.get("result").and_then(|r| r.as_str())
            {
                let message = message.trim();
                if !message.is_empty() {
                    return Ok(Some(message.to_string()));
                }
            }

            Ok(None)
        }
        _ => Ok(None), // Silently ignore errors to maintain graceful fallback
    }
}
