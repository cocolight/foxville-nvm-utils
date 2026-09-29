// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 cocolight
//
// deflate.rs -- 手写 DEFLATE(inflate) 与 gzip 解压。
//
// 目的：让 foxflash 能**直接读 .zip / .tar.gz 里的镜像而不用先解压**，
// 同时保持「零第三方依赖」——不引入 flate2 / miniz_oxide 之类的 crate。
//
// 覆盖范围：stored / fixed huffman / dynamic huffman 三种块（BTYPE 0/1/2），
// 不支持 BTYPE=3、zip64、加密。对本用途（解 Intel 官方固件包）足够。

#![allow(dead_code)]

/// 解压结果上限，防止恶意/损坏的流把内存吃光（256MB 远大于任何固件包）
const MAX_OUTPUT: usize = 256 * 1024 * 1024;

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bits: u64,
    nbits: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> BitReader<'a> {
        BitReader {
            data,
            pos: 0,
            bits: 0,
            nbits: 0,
        }
    }

    fn need(&mut self, n: u32) -> Result<(), String> {
        while self.nbits < n {
            if self.pos >= self.data.len() {
                return Err("数据流提前结束".to_string());
            }
            self.bits |= (self.data[self.pos] as u64) << self.nbits;
            self.pos += 1;
            self.nbits += 8;
        }
        Ok(())
    }

    fn bits(&mut self, n: u32) -> Result<u32, String> {
        if n == 0 {
            return Ok(0);
        }
        self.need(n)?;
        let v = (self.bits & ((1u64 << n) - 1)) as u32;
        self.bits >>= n;
        self.nbits -= n;
        Ok(v)
    }

    fn align(&mut self) {
        let drop = self.nbits % 8;
        self.bits >>= drop;
        self.nbits -= drop;
    }
}

struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    fn decode(&self, br: &mut BitReader) -> Result<u16, String> {
        let mut code: i32 = 0;
        let mut first: i32 = 0;
        let mut index: i32 = 0;
        for len in 1..16usize {
            code |= br.bits(1)? as i32;
            let count = self.counts[len] as i32;
            if code - first < count {
                return Ok(self.symbols[(index + code - first) as usize]);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err("非法 Huffman 编码".to_string())
    }
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
// code length code 的读取顺序（RFC1951 规定，不是 0..18 顺序）
const CL_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

fn build_huffman(lengths: &[u8]) -> Result<Huffman, String> {
    let mut counts = [0u16; 16];
    for &l in lengths.iter() {
        counts[l as usize] += 1;
    }
    counts[0] = 0;
    let mut offs = [0u16; 16];
    for len in 1..15usize {
        offs[len + 1] = offs[len] + counts[len];
    }
    let mut symbols = vec![0u16; lengths.len()];
    for (sym, &l) in lengths.iter().enumerate() {
        if l != 0 {
            symbols[offs[l as usize] as usize] = sym as u16;
            offs[l as usize] += 1;
        }
    }
    Ok(Huffman { counts, symbols })
}

pub fn inflate(data: &[u8], cap: usize) -> Result<Vec<u8>, String> {
    let mut br = BitReader::new(data);
    let mut out: Vec<u8> = Vec::new();

    loop {
        let last = br.bits(1)?;
        let btype = br.bits(2)?;

        match btype {
            0 => {
                // 未压缩块
                br.align();
                let len = br.bits(16)? as usize;
                let _nlen = br.bits(16)?;
                for _ in 0..len {
                    if br.pos >= br.data.len() {
                        return Err("stored 块数据不足".to_string());
                    }
                    out.push(br.data[br.pos]);
                    br.pos += 1;
                }
            }
            1 | 2 => {
                let (lit, dist) = if btype == 1 {
                    // 固定 Huffman：码长表是写死的
                    let mut l = vec![8u8; 288];
                    for i in 144..256 {
                        l[i] = 9;
                    }
                    for i in 256..280 {
                        l[i] = 7;
                    }
                    for i in 280..288 {
                        l[i] = 8;
                    }
                    let d = vec![5u8; 30];
                    (l, d)
                } else {
                    // 动态 Huffman：先读码长表
                    let hlit = br.bits(5)? as usize + 257;
                    let hdist = br.bits(5)? as usize + 1;
                    let hclen = br.bits(4)? as usize + 4;
                    let mut cl = [0u8; 19];
                    for i in 0..hclen {
                        cl[CL_ORDER[i]] = br.bits(3)? as u8;
                    }
                    let clh = build_huffman(&cl)?;
                    let mut lengths = Vec::with_capacity(hlit + hdist);
                    let total = hlit + hdist;
                    while lengths.len() < total {
                        let sym = clh.decode(&mut br)? as usize;
                        match sym {
                            0..=15 => lengths.push(sym as u8),
                            16 => {
                                if lengths.is_empty() {
                                    return Err("Huffman 重复码缺少前值".to_string());
                                }
                                let prev = *lengths.last().unwrap();
                                let rep = 3 + br.bits(2)? as usize;
                                for _ in 0..rep {
                                    lengths.push(prev);
                                }
                            }
                            17 => {
                                let rep = 3 + br.bits(3)? as usize;
                                for _ in 0..rep {
                                    lengths.push(0);
                                }
                            }
                            18 => {
                                let rep = 11 + br.bits(7)? as usize;
                                for _ in 0..rep {
                                    lengths.push(0);
                                }
                            }
                            _ => return Err("非法 code length symbol".to_string()),
                        }
                    }
                    if lengths.len() > total {
                        lengths.truncate(total);
                    }
                    let dl = lengths[hlit..].to_vec();
                    let ll = lengths[..hlit].to_vec();
                    (ll, dl)
                };

                let lh = build_huffman(&lit)?;
                let dh = build_huffman(&dist)?;

                loop {
                    let sym = lh.decode(&mut br)? as usize;
                    if sym < 256 {
                        out.push(sym as u8);
                    } else if sym == 256 {
                        break;
                    } else {
                        let idx = sym - 257;
                        if idx >= 29 {
                            return Err("非法长度符号".to_string());
                        }
                        let len = LEN_BASE[idx] as usize + br.bits(LEN_EXTRA[idx] as u32)? as usize;
                        let dsym = dh.decode(&mut br)? as usize;
                        if dsym >= 30 {
                            return Err("非法距离符号".to_string());
                        }
                        let dist =
                            DIST_BASE[dsym] as usize + br.bits(DIST_EXTRA[dsym] as u32)? as usize;
                        if dist > out.len() {
                            return Err("距离超出已输出窗口".to_string());
                        }
                        let start = out.len() - dist;
                        for k in 0..len {
                            let b = out[start + k];
                            out.push(b);
                        }
                    }
                    if out.len() > cap {
                        return Err("解压结果超出上限".to_string());
                    }
                }
            }
            _ => return Err("不支持的 deflate 块类型 (BTYPE=3)".to_string()),
        }

        if out.len() > cap {
            return Err("解压结果超出上限".to_string());
        }
        if last == 1 {
            break;
        }
    }
    Ok(out)
}

/// gzip：只处理单成员（固件包不会用多成员）
pub fn gunzip(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 18 || data[0] != 0x1f || data[1] != 0x8b {
        return Err("不是 gzip 数据".to_string());
    }
    if data[2] != 8 {
        return Err("gzip 压缩方法不是 deflate".to_string());
    }
    let flg = data[3];
    let mut p: usize = 10;
    if flg & 0x04 != 0 {
        // FEXTRA
        if p + 2 > data.len() {
            return Err("gzip 头损坏".to_string());
        }
        let xlen = u16::from_le_bytes([data[p], data[p + 1]]) as usize;
        p += 2 + xlen;
    }
    // FNAME / FCOMMENT / FHCRC：都是 NUL 结尾的串
    for &mask in [0x08u8, 0x10, 0x02].iter() {
        if flg & mask != 0 {
            while p < data.len() && data[p] != 0 {
                p += 1;
            }
            p += 1;
        }
    }
    if p >= data.len() {
        return Err("gzip 头损坏".to_string());
    }
    inflate(&data[p..], MAX_OUTPUT)
}
