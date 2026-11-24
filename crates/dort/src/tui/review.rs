//! File reviews built from typed plans.
pub fn effects_report(report: &str) -> String {
    let mut lines = report.lines();
    let mut ordered = String::new();
    if let Some(heading) = lines.next() {
        ordered.push_str(heading);
        ordered.push('\n');
    }
    let (warnings, effects): (Vec<_>, Vec<_>) =
        lines.partition(|line| line.starts_with("Warning: "));
    let mut shown = std::collections::HashSet::new();
    for line in warnings
        .into_iter()
        .filter(|line| shown.insert(*line))
        .chain(effects)
    {
        ordered.push_str(line);
        ordered.push('\n');
    }
    ordered
}
