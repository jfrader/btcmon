pub fn commas(value: u64) -> String {
    let raw = value.to_string();
    let mut out = String::new();
    for (index, ch) in raw.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

pub fn short_hash(hash: &str) -> String {
    let chars: Vec<char> = hash.chars().collect();
    if chars.len() <= 17 {
        return hash.to_string();
    }
    format!(
        "{}…{}",
        chars.iter().take(8).collect::<String>(),
        chars.iter().rev().take(8).rev().collect::<String>()
    )
}

pub fn bytes_compact(bytes: u64) -> String {
    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;
    if bytes >= GIB {
        format!("{} GB", commas(bytes / GIB))
    } else if bytes >= MIB {
        format!("{} MB", commas(bytes / MIB))
    } else {
        format!("{} B", commas(bytes))
    }
}

pub fn sparkline(values: &[f64], width: usize) -> String {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    if width == 0 || values.is_empty() {
        return String::new();
    }

    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = max - min;

    (0..width)
        .map(|column| {
            let index = if width == 1 {
                values.len() - 1
            } else {
                column * (values.len() - 1) / (width - 1)
            };
            let value = values[index];
            if !value.is_finite() {
                return ' ';
            }
            let rank = if span <= f64::EPSILON {
                4
            } else {
                (((value - min) / span) * 7.0).round() as usize
            };
            BARS[rank.min(7)]
        })
        .collect()
}

pub fn chain_label(chain: &str) -> &str {
    match chain {
        "bitcoin" | "main" => "main",
        "testnet" | "test" => "test",
        "testnet4" => "test4",
        "signet" => "signet",
        "regtest" => "regtest",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::{bytes_compact, commas, short_hash, sparkline};

    #[test]
    fn commas_groups_thousands() {
        assert_eq!(commas(109_432), "109,432");
        assert_eq!(commas(12), "12");
    }

    #[test]
    fn short_hash_keeps_head_and_tail() {
        assert_eq!(
            short_hash("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"),
            "01234567…89abcdef"
        );
        assert_eq!(short_hash("abc"), "abc");
    }

    #[test]
    fn sparkline_is_the_requested_width() {
        let line = sparkline(&[1.0, 2.0, 3.0, 2.5], 8);
        assert_eq!(line.chars().count(), 8);
        assert!(line.chars().all(|ch| "▁▂▃▄▅▆▇█".contains(ch)));
    }

    #[test]
    fn bytes_compact_uses_gb_for_chainstate() {
        assert_eq!(bytes_compact(687 * 1024 * 1024 * 1024), "687 GB");
    }
}
