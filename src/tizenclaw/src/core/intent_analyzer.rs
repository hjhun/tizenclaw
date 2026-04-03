use regex::Regex;

pub struct IntentAnalyzer;

impl IntentAnalyzer {
    /// Determines if a prompt requires a multi-step Plan-and-Solve cognitive capability
    /// by structurally analyzing typical patterns (e.g. numbered lists, bullet points, punctuation density).
    /// This removes previous language-specific hardcoded values.
    pub fn is_complex_task(prompt: &str) -> bool {
        let mut complexity_score = 0;

        // 1. List Detection (Numbered or bullet points)
        // Checks for lines starting with "1.", "a)", "-", "*", "1)", etc.
        if let Ok(list_pattern) = Regex::new(r"(?m)^\s*(?:\d+[\.\)]|[-*])\s+") {
            let list_count = list_pattern.find_iter(prompt).count();
            if list_count >= 2 {
                complexity_score += 3; // Very strong indicator of multi-step task
            } else if list_count == 1 {
                complexity_score += 1;
            }
        }

        // 2. Sentence Density / Punctuation
        // Count typical sentence terminators and multi-clause connectors
        let punct_count = prompt.chars().filter(|c| ['.', '?', '!', '。', '？', '！', ';'].contains(c)).count();
        if punct_count >= 3 {
            complexity_score += 2;
        } else if punct_count == 2 {
            complexity_score += 1;
        }

        // 3. Newline Density
        let newline_count = prompt.chars().filter(|c| *c == '\n').count();
        if newline_count >= 2 {
            complexity_score += 1;
        }

        // 4. Content Length
        if prompt.len() > 100 {
            complexity_score += 1;
        }

        // Threshold evaluation (Score 3+ triggers complex planning)
        complexity_score >= 3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_prompt() {
        assert!(!IntentAnalyzer::is_complex_task("Turn off the light."));
        assert!(!IntentAnalyzer::is_complex_task("불 꺼주세요."));
        assert!(!IntentAnalyzer::is_complex_task("What is the time?"));
    }

    #[test]
    fn test_list_prompt() {
        let prompt = "Please do the following:\n1. Turn on the TV\n2. Set volume to 20\n3. Launch Netflix";
        assert!(IntentAnalyzer::is_complex_task(prompt));
        
        let prompt_bullet = "- Play music\n- Turn off lights\n- Lock the door";
        assert!(IntentAnalyzer::is_complex_task(prompt_bullet));
    }

    #[test]
    fn test_dense_punctuation() {
        let prompt = "Turn on the AC. Then close the door! And what about the weather?";
        assert!(IntentAnalyzer::is_complex_task(prompt));
        
        // Korean multi-sentence
        let prompt_kr = "에어컨 켜줘. 그리고 문도 닫아줄래? 날씨는 어때.";
        assert!(IntentAnalyzer::is_complex_task(prompt_kr));
    }
}
