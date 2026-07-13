use crate::Result;

use super::canvas::Canvas;

pub(crate) const CELL_WIDTH: u32 = 6;
const ELLIPSIS_NOTICE: &str = "... SEE MANIFEST ...";

pub(crate) fn escape_text(value: &str) -> String {
    value.chars().flat_map(char::escape_default).collect()
}

pub(crate) fn ellipsize(value: &str, max_cells: usize) -> String {
    let escaped = escape_text(value);
    let chars = escaped.chars().collect::<Vec<_>>();
    if chars.len() <= max_cells {
        return escaped;
    }
    let notice = ELLIPSIS_NOTICE.chars().collect::<Vec<_>>();
    if max_cells <= notice.len() + 2 {
        return chars.into_iter().take(max_cells).collect();
    }
    let retained = max_cells - notice.len();
    let prefix = retained.div_ceil(2);
    let suffix = retained / 2;
    chars[..prefix]
        .iter()
        .chain(notice.iter())
        .chain(chars[chars.len() - suffix..].iter())
        .collect()
}

pub(crate) fn draw_text(
    canvas: &mut Canvas,
    x: u32,
    y: u32,
    text: &str,
    color: [u8; 3],
) -> Result<()> {
    for (index, character) in text.chars().enumerate() {
        let cell_x = x + u32::try_from(index)
            .unwrap_or(u32::MAX)
            .saturating_mul(CELL_WIDTH);
        draw_glyph(canvas, cell_x, y, character, color)?;
    }
    Ok(())
}

fn draw_glyph(canvas: &mut Canvas, x: u32, y: u32, character: char, color: [u8; 3]) -> Result<()> {
    let rows = glyph(character);
    for (row, bits) in rows.into_iter().enumerate() {
        for column in 0..5_u32 {
            if bits & (0b1_0000 >> column) != 0 {
                canvas.set_pixel(x + column, y + 1 + row as u32, color)?;
            }
        }
    }
    Ok(())
}

// Fixed 5x7 glyphs drawn into a 6x10 cell. Lowercase deliberately shares the
// uppercase raster so caller text remains readable without host font shaping.
fn glyph(character: char) -> [u8; 7] {
    match character.to_ascii_uppercase() {
        ' ' => [0, 0, 0, 0, 0, 0, 0],
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [28, 18, 17, 17, 17, 18, 28],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [31, 4, 4, 4, 4, 4, 31],
        'J' => [7, 2, 2, 2, 18, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 31],
        '+' => [0, 4, 4, 31, 4, 4, 0],
        '=' => [0, 0, 31, 0, 31, 0, 0],
        '>' => [16, 8, 4, 2, 4, 8, 16],
        '<' => [1, 2, 4, 8, 4, 2, 1],
        ':' => [0, 4, 4, 0, 4, 4, 0],
        ';' => [0, 4, 4, 0, 4, 4, 8],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        ',' => [0, 0, 0, 0, 4, 4, 8],
        '/' => [1, 2, 2, 4, 8, 8, 16],
        '\\' => [16, 8, 8, 4, 2, 2, 1],
        '(' => [2, 4, 8, 8, 8, 4, 2],
        ')' => [8, 4, 2, 2, 2, 4, 8],
        '[' => [14, 8, 8, 8, 8, 8, 14],
        ']' => [14, 2, 2, 2, 2, 2, 14],
        '{' => [2, 4, 4, 8, 4, 4, 2],
        '}' => [8, 4, 4, 2, 4, 4, 8],
        '!' => [4, 4, 4, 4, 4, 0, 4],
        '?' => [14, 17, 1, 2, 4, 0, 4],
        '#' => [10, 31, 10, 10, 31, 10, 0],
        '%' => [17, 2, 4, 8, 17, 0, 0],
        '&' => [12, 18, 20, 8, 21, 18, 13],
        '*' => [0, 21, 14, 31, 14, 21, 0],
        '"' => [10, 10, 0, 0, 0, 0, 0],
        '\'' => [4, 4, 0, 0, 0, 0, 0],
        '|' => [4, 4, 4, 4, 4, 4, 4],
        '^' => [4, 10, 17, 0, 0, 0, 0],
        '~' => [0, 0, 9, 22, 0, 0, 0],
        '@' => [14, 17, 23, 21, 23, 16, 14],
        '$' => [4, 15, 20, 14, 5, 30, 4],
        '`' => [8, 4, 0, 0, 0, 0, 0],
        _ => [14, 17, 1, 2, 4, 0, 4],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_and_middle_notice_are_platform_independent() {
        assert_eq!(escape_text("a\nλ"), "a\\n\\u{3bb}");
        let shortened = ellipsize("abcdefghijklmnopqrstuvwxyz", 24);
        assert_eq!(shortened.chars().count(), 24);
        assert!(shortened.contains("SEE MANIFEST"));
        assert_eq!(ellipsize("short", 24), "short");
    }
}
