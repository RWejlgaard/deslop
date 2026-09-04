/// Split markdown into paragraph chunks on blank lines.
/// Fenced code blocks (``` or ~~~) are never split, even if they
/// contain blank lines.
pub fn split_chunks(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut fence: Option<(char, usize)> = None;

    for line in text.lines() {
        let trimmed = line.trim_start();

        if let Some((marker, min_len)) = fence {
            current.push(line);
            let run = trimmed.chars().take_while(|&c| c == marker).count();
            if run >= min_len {
                fence = None;
            }
            continue;
        }

        let first = trimmed.chars().next();
        if first == Some('`') || first == Some('~') {
            let marker = first.unwrap();
            let run = trimmed.chars().take_while(|&c| c == marker).count();
            if run >= 3 {
                fence = Some((marker, run));
                current.push(line);
                continue;
            }
        }

        if line.trim().is_empty() {
            if !current.is_empty() {
                chunks.push(current.join("\n"));
                current.clear();
            }
        } else {
            current.push(line);
        }
    }

    if !current.is_empty() {
        chunks.push(current.join("\n"));
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_blank_lines() {
        let chunks = split_chunks("# Title\n\npara one\nstill one\n\npara two\n");
        assert_eq!(chunks, vec!["# Title", "para one\nstill one", "para two"]);
    }

    #[test]
    fn keeps_code_fence_whole() {
        let chunks = split_chunks("before\n\n```rust\nlet a = 1;\n\nlet b = 2;\n```\n\nafter\n");
        assert_eq!(chunks.len(), 3);
        assert!(chunks[1].contains("let a = 1;\n\nlet b = 2;"));
    }

    #[test]
    fn empty_input() {
        assert!(split_chunks("\n\n  \n").is_empty());
    }
}
