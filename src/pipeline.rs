use crate::audio::AudioInfo;
use rustfft::{num_complex::Complex, FftPlanner};
use std::f32::consts::PI;
use std::sync::mpsc::Sender;

pub const FFT_SIZE: usize = 2048;
pub const BANDS: usize = FFT_SIZE / 2 + 1;

#[derive(Clone, Copy, PartialEq)]
pub enum Channel { Left, Mix, Right, Split }

pub enum Msg {
    Info(AudioInfo),
    Column(usize, Vec<f32>),
    ColumnR(usize, Vec<f32>), // R channel, only sent when Channel::Split
    Done,
    Error(String),
}

pub fn start(
    left: Vec<f32>,
    right: Vec<f32>,
    info: AudioInfo,
    num_cols: usize,
    overlap: f32,
    channel: Channel,
    tx: Sender<Msg>,
) {
    std::thread::spawn(move || {
        tx.send(Msg::Info(info)).ok();

        // For Split, keep both channels; otherwise pick one
        let (samples_l, samples_r_opt): (Vec<f32>, Option<Vec<f32>>) = match channel {
            Channel::Left  => (left, None),
            Channel::Right => (right, None),
            Channel::Mix   => (
                left.iter().zip(right.iter()).map(|(&l, &r)| (l + r) * 0.5).collect(),
                None,
            ),
            Channel::Split => (left, Some(right)),
        };

        let total = samples_l.len();
        if total < FFT_SIZE {
            tx.send(Msg::Error("Audio too short".into())).ok();
            return;
        }

        let hop = ((FFT_SIZE as f32 * (1.0 - overlap)).round() as usize).max(1);
        let num_frames = (total - FFT_SIZE) / hop + 1;

        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);

        let hann: Vec<f32> = (0..FFT_SIZE)
            .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / (FFT_SIZE - 1) as f32).cos()))
            .collect();

        let mut col_pow_l: Vec<Vec<f64>> = vec![vec![0.0; BANDS]; num_cols];
        let mut col_pow_r: Option<Vec<Vec<f64>>> = samples_r_opt
            .as_ref()
            .map(|_| vec![vec![0.0; BANDS]; num_cols]);
        let mut col_cnt: Vec<usize> = vec![0; num_cols];
        let mut next_send: usize = 0;
        let mut buf = vec![Complex::new(0.0f32, 0.0); FFT_SIZE];

        for frame in 0..num_frames {
            let col = if num_frames > 1 {
                (frame * num_cols / num_frames).min(num_cols - 1)
            } else {
                0
            };

            while next_send < col {
                let bl = finalize_col(&col_pow_l[next_send], col_cnt[next_send]);
                if tx.send(Msg::Column(next_send, bl)).is_err() { return; }
                if let Some(ref pr) = col_pow_r {
                    let br = finalize_col(&pr[next_send], col_cnt[next_send]);
                    if tx.send(Msg::ColumnR(next_send, br)).is_err() { return; }
                }
                next_send += 1;
            }

            let s0 = (frame * hop).min(total - FFT_SIZE);

            // L (or only) channel
            for i in 0..FFT_SIZE {
                buf[i] = Complex::new(samples_l[s0 + i] * hann[i], 0.0);
            }
            fft.process(&mut buf);
            accumulate_power(&mut col_pow_l[col], &buf);

            // R channel (Split only)
            if let (Some(ref rs), Some(ref mut pr)) = (&samples_r_opt, &mut col_pow_r) {
                for i in 0..FFT_SIZE {
                    buf[i] = Complex::new(rs[s0 + i] * hann[i], 0.0);
                }
                fft.process(&mut buf);
                accumulate_power(&mut pr[col], &buf);
            }

            col_cnt[col] += 1;
        }

        while next_send < num_cols {
            let bl = finalize_col(&col_pow_l[next_send], col_cnt[next_send]);
            if tx.send(Msg::Column(next_send, bl)).is_err() { return; }
            if let Some(ref pr) = col_pow_r {
                let br = finalize_col(&pr[next_send], col_cnt[next_send]);
                if tx.send(Msg::ColumnR(next_send, br)).is_err() { return; }
            }
            next_send += 1;
        }

        tx.send(Msg::Done).ok();
    });
}

fn accumulate_power(pow: &mut [f64], buf: &[Complex<f32>]) {
    let n2 = (FFT_SIZE * FFT_SIZE) as f64;
    for b in 0..BANDS {
        let (re, im): (f64, f64) = match b {
            0 => (buf[0].re as f64, 0.0),
            b if b == BANDS - 1 => (buf[FFT_SIZE / 2].re as f64, 0.0),
            _ => (buf[b].re as f64, buf[b].im as f64),
        };
        pow[b] += (re * re + im * im) / n2;
    }
}

fn finalize_col(pow: &[f64], count: usize) -> Vec<f32> {
    let n = count.max(1) as f64;
    pow.iter()
        .map(|&p| (10.0 * (p / n).max(1e-28).log10()) as f32)
        .collect()
}
