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

    let samples = sample_series(values, width);
    let min = samples.iter().copied().fold(f64::INFINITY, f64::min);
    let max = samples.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = max - min;

    samples
        .into_iter()
        .map(|value| {
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

pub fn line_chart(values: &[f64], width: usize, height: usize) -> Vec<String> {
    if height == 0 {
        return Vec::new();
    }
    if width == 0 || values.is_empty() {
        return vec![" ".repeat(width); height];
    }

    let samples = sample_series(values, width);
    let min = samples.iter().copied().fold(f64::INFINITY, f64::min);
    let max = samples.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = max - min;
    let levels = height * 2;
    let ys: Vec<usize> = samples
        .iter()
        .map(|value| {
            if !value.is_finite() || span <= f64::EPSILON {
                return (levels - 1) / 2;
            }
            let rank = ((value - min) / span) * (levels - 1) as f64;
            rank.round().clamp(0.0, (levels - 1) as f64) as usize
        })
        .collect();

    let mut pixels = vec![vec![false; width]; levels];
    for x in 0..width {
        let y = ys[x];
        if x > 0 {
            let prev = ys[x - 1];
            let lo = prev.min(y);
            let hi = prev.max(y);
            for yy in lo..=hi {
                pixels[levels - 1 - yy][x] = true;
            }
        } else {
            pixels[levels - 1 - y][x] = true;
        }
    }

    let mut rows = Vec::with_capacity(height);
    for row in 0..height {
        let mut line = String::with_capacity(width);
        for x in 0..width {
            let top = pixels[row * 2][x];
            let bottom = pixels[row * 2 + 1][x];
            line.push(match (top, bottom) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => ' ',
            });
        }
        rows.push(line);
    }

    let last_x = width - 1;
    let last_row = (levels - 1 - ys[last_x]) / 2;
    if let Some(line) = rows.get_mut(last_row) {
        let mut chars: Vec<char> = line.chars().collect();
        if last_x < chars.len() {
            chars[last_x] = '*';
            *line = chars.into_iter().collect();
        }
    }
    rows
}

fn sample_series(values: &[f64], width: usize) -> Vec<f64> {
    (0..width)
        .map(|column| {
            let index = if width == 1 {
                values.len() - 1
            } else {
                column * (values.len() - 1) / (width - 1)
            };
            values[index]
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
    use super::{bytes_compact, commas, line_chart, short_hash, sparkline};

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
    fn line_chart_uses_half_blocks_and_marks_the_latest_point() {
        let rows = line_chart(&[1.0, 2.0, 3.0, 4.0], 8, 4);
        assert_eq!(rows.len(), 4);
        assert!(rows.iter().all(|row| row.chars().count() == 8));
        assert!(rows.iter().all(|row| {
            row.chars()
                .all(|ch| matches!(ch, ' ' | '▀' | '▄' | '█' | '*'))
        }));
        assert!(rows.iter().any(|row| row.contains('*')));
        let first_ink = rows
            .iter()
            .rev()
            .find_map(|row| row.chars().position(|ch| ch != ' '));
        let last_ink = rows.iter().find_map(|row| {
            row.chars()
                .rev()
                .position(|ch| ch != ' ')
                .map(|from_end| row.chars().count() - 1 - from_end)
        });
        assert!(first_ink.unwrap() < last_ink.unwrap());
    }

    #[test]
    fn bytes_compact_uses_gb_for_chainstate() {
        assert_eq!(bytes_compact(687 * 1024 * 1024 * 1024), "687 GB");
    }
}
