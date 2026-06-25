pub fn sox(level: f64) -> [u8; 4] {
    use std::f64::consts::PI;
    let r = if level >= 0.13 && level < 0.73 {
        ((level - 0.13) / 0.60 * PI / 2.0).sin()
    } else if level >= 0.73 {
        1.0
    } else {
        0.0
    };
    let g = if level >= 0.6 && level < 0.91 {
        ((level - 0.6) / 0.31 * PI / 2.0).sin()
    } else if level >= 0.91 {
        1.0
    } else {
        0.0
    };
    let b = if level < 0.60 {
        0.5 * (level / 0.6 * PI).sin()
    } else if level >= 0.78 {
        (level - 0.78) / 0.22
    } else {
        0.0
    };
    [
        (r * 255.0 + 0.5) as u8,
        (g * 255.0 + 0.5) as u8,
        (b * 255.0 + 0.5) as u8,
        255,
    ]
}
