/// Plan-and-solve sub-goal decomposition for complex or comparative queries.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub goals: Vec<String>,
}

/// Analyzes the query and generates a step-by-step plan if the query is multi-part or comparative.
/// Returns None if the query is simple and direct (avoiding unnecessary overhead).
pub fn analyze_and_plan(query: &str) -> Option<Plan> {
    let lower = query.to_lowercase();
    let trimmed = query.trim();

    // 1. Comparative queries (e.g. "compare A and B", "difference between X and Y", "A vs B")
    if lower.contains("compare ") || lower.contains("contrast ") || lower.contains("difference between") || lower.contains(" vs ") || lower.contains(" versus ") {
        let (topic_a, topic_b) = extract_comparison_targets(&lower, trimmed);
        let goals = vec![
            format!("Retrieve information regarding {topic_a}"),
            format!("Retrieve information regarding {topic_b}"),
            "Synthesize comparative analysis and contrast key points".to_string(),
        ];
        return Some(Plan { goals });
    }

    // 2. Multi-part queries with multiple question marks (e.g. "What is X? How does Y work?")
    let q_count = trimmed.matches('?').count();
    if q_count >= 2 {
        let parts: Vec<&str> = trimmed.split('?').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        if parts.len() >= 2 {
            let mut goals = Vec::new();
            for (idx, p) in parts.iter().enumerate() {
                let clean = p.trim_start_matches("and ").trim();
                goals.push(format!("Sub-query {}: {clean}", idx + 1));
            }
            goals.push("Consolidate findings into unified response".to_string());
            return Some(Plan { goals });
        }
    }

    // 3. Multi-aspect queries with "and also", "as well as", "along with"
    let conjunctions = [" and also ", " as well as ", " along with ", " in addition to "];
    for conj in conjunctions {
        if let Some(pos) = lower.find(conj) {
            let part1 = trimmed[..pos].trim();
            let part2 = trimmed[pos + conj.len()..].trim().trim_end_matches('?').trim();
            if part1.len() > 5 && part2.len() > 5 {
                let goals = vec![
                    format!("Investigate primary topic: {part1}"),
                    format!("Investigate related aspect: {part2}"),
                    "Integrate both dimensions with supporting citations".to_string(),
                ];
                return Some(Plan { goals });
            }
        }
    }

    // 4. Analytical + Calculation query
    if (lower.contains("calculate") || lower.contains("compute") || lower.contains("growth rate") || lower.contains("margin"))
        && (lower.contains("from the ") || lower.contains("in the ") || lower.contains("based on"))
    {
        let goals = vec![
            "Retrieve baseline financial / numerical figures from document sources".to_string(),
            "Run deterministic calculation on verified figures".to_string(),
            "Synthesize analysis with grounded citations".to_string(),
        ];
        return Some(Plan { goals });
    }

    None
}

fn extract_comparison_targets(lower: &str, original: &str) -> (String, String) {
    if let Some(pos) = lower.find("difference between ") {
        let remainder = &original[pos + "difference between ".len()..];
        if let Some((a, b)) = split_by_conjunction(remainder) {
            return (a, b);
        }
    }

    if let Some(pos) = lower.find("compare ") {
        let remainder = &original[pos + "compare ".len()..];
        if let Some((a, b)) = split_by_conjunction(remainder) {
            return (a, b);
        }
    }

    if let Some(pos) = lower.find(" vs ") {
        let a = original[..pos].trim().trim_start_matches("compare ").trim();
        let b = original[pos + 4..].trim().trim_end_matches('?').trim();
        return (a.to_string(), b.to_string());
    }

    if let Some(pos) = lower.find(" versus ") {
        let a = original[..pos].trim().trim_start_matches("compare ").trim();
        let b = original[pos + 8..].trim().trim_end_matches('?').trim();
        return (a.to_string(), b.to_string());
    }

    ("primary topic".to_string(), "comparison topic".to_string())
}

fn split_by_conjunction(text: &str) -> Option<(String, String)> {
    let clean = text.trim_end_matches('?').trim();
    if let Some(pos) = clean.to_lowercase().find(" and ") {
        let a = clean[..pos].trim().to_string();
        let b = clean[pos + 5..].trim().to_string();
        if !a.is_empty() && !b.is_empty() {
            return Some((a, b));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_comparative_query_planning() {
        let plan = analyze_and_plan("Compare Enterprise plan and Starter plan?").unwrap();
        assert_eq!(plan.goals.len(), 3);
        assert!(plan.goals[0].contains("Enterprise plan"));
        assert!(plan.goals[1].contains("Starter plan"));
        assert!(plan.goals[2].contains("Synthesize"));
    }

    #[test]
    fn test_multi_question_planning() {
        let plan = analyze_and_plan("What is the refund deadline? How do I request a chargeback?").unwrap();
        assert_eq!(plan.goals.len(), 3);
        assert!(plan.goals[0].contains("refund deadline"));
        assert!(plan.goals[1].contains("chargeback"));
    }

    #[test]
    fn test_simple_query_no_plan() {
        assert_eq!(analyze_and_plan("What is the refund policy?"), None);
        assert_eq!(analyze_and_plan("Summarize the document"), None);
    }
}
