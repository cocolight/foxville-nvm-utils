// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 cocolight
//
// md5.rs -- MD5（标准库没有，手写一份，避免引入外部 crate）
//
// 仅 foxflash 使用：给镜像算摘要，方便核对「这份 .bin 是不是我看的那份」。
// zlib / gzip / zip 里的 MD5 用不到，只是为了输出给人看。

#![allow(dead_code)]

const MD5_S: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21,
    6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

const MD5_K: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
    0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
    0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
    0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
    0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
    0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
    0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
    0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
];

fn rotl(x: u32, n: u32) -> u32 {
    (x << n) | (x >> (32 - n))
}

struct Md5 {
    state: [u32; 4],
    buf: Vec<u8>,
    len: u64,
}

impl Md5 {
    fn new() -> Md5 {
        Md5 {
            state: [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476],
            buf: Vec::with_capacity(64),
            len: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.len += data.len() as u64;
        self.buf.extend_from_slice(data);
        while self.buf.len() >= 64 {
            let block: Vec<u8> = self.buf.drain(..64).collect();
            Self::process(&mut self.state, &block);
        }
    }

    fn finish(mut self) -> String {
        let bit_len = self.len * 8;
        let mut tail = Vec::new();
        tail.push(0x80u8);
        // 补零到满足 (have + 1 + zeros + 8) % 64 == 0
        let have = self.buf.len();
        let need = (56isize - (have as isize + 1)).rem_euclid(64) as usize;
        tail.extend_from_slice(&vec![0u8; need]);
        tail.extend_from_slice(&bit_len.to_le_bytes());

        let mut padded: Vec<u8> = Vec::with_capacity(have + tail.len() + 64);
        padded.extend_from_slice(&self.buf);
        padded.extend_from_slice(&tail);
        let mut i = 0usize;
        while i + 64 <= padded.len() {
            Self::process(&mut self.state, &padded[i..i + 64]);
            i += 64;
        }

        let mut out = String::new();
        for w in self.state.iter() {
            out.push_str(&format!(
                "{:02x}{:02x}{:02x}{:02x}",
                w & 0xff,
                (w >> 8) & 0xff,
                (w >> 16) & 0xff,
                (w >> 24) & 0xff
            ));
        }
        out
    }

    fn process(state: &mut [u32; 4], block: &[u8]) {
        let mut m = [0u32; 16];
        for i in 0..16 {
            m[i] = u32::from_le_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        let (mut a, mut b, mut c, mut d) = (state[0], state[1], state[2], state[3]);
        for i in 0..64usize {
            let (f, g) = match i / 16 {
                0 => (((b & c) | ((!b) & d)), i),
                1 => (((d & b) | ((!d) & c)), (5 * i + 1) % 16),
                2 => ((b ^ c ^ d), (3 * i + 5) % 16),
                _ => ((c ^ (b | (!d))), (7 * i) % 16),
            };
            let tmp = a
                .wrapping_add(f)
                .wrapping_add(MD5_K[i])
                .wrapping_add(m[g]);
            let new_b = b.wrapping_add(rotl(tmp, MD5_S[i]));
            a = d;
            d = c;
            c = b;
            b = new_b;
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
    }
}

pub fn md5_hex(data: &[u8]) -> String {
    let mut h = Md5::new();
    h.update(data);
    h.finish()
}
