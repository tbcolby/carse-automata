//! Binary records. Every record starts with `CRS` + a kind byte + a version
//! byte, and every decoder validates what it reads: lengths, cell states and
//! stored counts. 0.1 had no header and trusted stored bytes (a cell value
//! above 6 panicked in the status line at every launch). 0.1 records are still
//! read (marked "0.1" below) and rewritten in the new format on the next save.

use crate::grid::{Grid, CELL_COUNT};
use crate::rules::DeathReason;
use crate::seed::TouchMap;

pub const VERSION: u8 = 1;
const MAGIC: &[u8; 3] = b"CRS";
const HDR: usize = 5;

const KIND_TOUCH: u8 = b'T';
const KIND_EPOCH: u8 = b'E';
const KIND_META: u8 = b'M';
const KIND_FOSSIL: u8 = b'F';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// Not a record of this kind (wrong magic or kind).
    Format,
    /// Shorter or longer than the format allows.
    Length,
    /// A value out of range (for example a cell state above 6).
    Value,
    /// Written by a newer version of the app: don't overwrite it.
    NewerVersion(u8),
}

fn header(kind: u8) -> Vec<u8> {
    let mut v = Vec::with_capacity(64);
    v.extend_from_slice(MAGIC);
    v.push(kind);
    v.push(VERSION);
    v
}

fn body(bytes: &[u8], kind: u8) -> Result<&[u8], DecodeError> {
    if bytes.len() < HDR || &bytes[..3] != MAGIC || bytes[3] != kind {
        return Err(DecodeError::Format);
    }
    if bytes[4] > VERSION {
        return Err(DecodeError::NewerVersion(bytes[4]));
    }
    Ok(&bytes[HDR..])
}

fn u32_at(b: &[u8], i: usize) -> u32 { u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) }

// ---- touch map ----

pub fn encode_touch(t: &TouchMap) -> Vec<u8> {
    let mut v = header(KIND_TOUCH);
    for c in t.counts() {
        v.extend_from_slice(&c.to_le_bytes());
    }
    v
}

pub fn decode_touch(bytes: &[u8]) -> Result<TouchMap, DecodeError> {
    let raw = match body(bytes, KIND_TOUCH) {
        Ok(b) => b,
        Err(DecodeError::Format) if bytes.len() == CELL_COUNT * 4 => bytes, // 0.1: bare array
        Err(e) => return Err(e),
    };
    if raw.len() != CELL_COUNT * 4 {
        return Err(DecodeError::Length);
    }
    TouchMap::from_vec(raw.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect())
        .ok_or(DecodeError::Length)
}

// ---- current epoch ----

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EpochState {
    pub epoch: u32,
    pub tick: u32,
    pub seed: u32,
    pub grid: Grid,
}

pub fn encode_epoch(epoch: u32, tick: u32, seed: u32, grid: &Grid) -> Vec<u8> {
    let mut v = header(KIND_EPOCH);
    v.extend_from_slice(&epoch.to_le_bytes());
    v.extend_from_slice(&tick.to_le_bytes());
    v.extend_from_slice(&seed.to_le_bytes());
    v.extend_from_slice(&grid.cells);
    v
}

pub fn decode_epoch(bytes: &[u8]) -> Result<EpochState, DecodeError> {
    let raw = match body(bytes, KIND_EPOCH) {
        Ok(b) => b,
        Err(DecodeError::Format) if bytes.len() == 12 + CELL_COUNT => bytes, // 0.1: no header
        Err(e) => return Err(e),
    };
    if raw.len() != 12 + CELL_COUNT {
        return Err(DecodeError::Length);
    }
    let grid = Grid::from_bytes(&raw[12..]).ok_or(DecodeError::Value)?;
    Ok(EpochState { epoch: u32_at(raw, 0), tick: u32_at(raw, 4), seed: u32_at(raw, 8), grid })
}

// ---- session meta ----

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Meta {
    /// Wall-clock ms (local time) when the app last saved; `None` if the clock wasn't set.
    pub last_wall_ms: Option<u64>,
}

pub fn encode_meta(m: &Meta) -> Vec<u8> {
    let mut v = header(KIND_META);
    v.extend_from_slice(&m.last_wall_ms.unwrap_or(0).to_le_bytes());
    v
}

pub fn decode_meta(bytes: &[u8]) -> Result<Meta, DecodeError> {
    match body(bytes, KIND_META) {
        Ok(raw) => {
            if raw.len() != 8 {
                return Err(DecodeError::Length);
            }
            let ms = u64::from_le_bytes([raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7]]);
            Ok(Meta { last_wall_ms: (ms != 0).then_some(ms) })
        }
        // 0.1 stored uptime, which is meaningless across boots.
        Err(DecodeError::Format) if bytes.len() == 12 => Ok(Meta { last_wall_ms: None }),
        Err(e) => Err(e),
    }
}

// ---- fossils ----

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Fossil {
    pub epoch: u32,
    pub tick_count: u32,
    pub seed: u32,
    pub death: Option<DeathReason>,
    pub energy: u32,
    pub grid: Grid,
}

/// Number of fossil slots (a ring: epoch N lives in slot N % 100).
pub const MAX_FOSSILS: u32 = 100;

pub fn fossil_key(epoch: u32) -> String { format!("fossil.{}", epoch % MAX_FOSSILS) }

pub fn encode_fossil(f: &Fossil) -> Vec<u8> {
    let rle = rle_compress(&f.grid.cells);
    let mut v = header(KIND_FOSSIL);
    v.extend_from_slice(&f.epoch.to_le_bytes());
    v.extend_from_slice(&f.tick_count.to_le_bytes());
    v.extend_from_slice(&f.seed.to_le_bytes());
    v.push(f.death.map(|d| d.as_u8()).unwrap_or(255));
    v.extend_from_slice(&f.energy.to_le_bytes());
    v.extend_from_slice(&(rle.len() as u32).to_le_bytes());
    v.extend_from_slice(&rle);
    v
}

pub fn decode_fossil(bytes: &[u8]) -> Result<Fossil, DecodeError> {
    let (raw, rle) = match body(bytes, KIND_FOSSIL) {
        Ok(b) => {
            if b.len() < 21 {
                return Err(DecodeError::Length);
            }
            let n = u32_at(b, 17) as usize;
            if b.len() - 21 != n {
                return Err(DecodeError::Length); // stale tail or truncated
            }
            (&b[..17], &b[21..])
        }
        // 0.1: 17-byte header then RLE to the end of the key.
        Err(DecodeError::Format) if bytes.len() >= 17 => (&bytes[..17], &bytes[17..]),
        Err(e) => return Err(e),
    };
    let cells = rle_decompress(rle, CELL_COUNT).ok_or(DecodeError::Value)?;
    let grid = Grid::from_bytes(&cells).ok_or(DecodeError::Value)?;
    Ok(Fossil {
        epoch: u32_at(raw, 0),
        tick_count: u32_at(raw, 4),
        seed: u32_at(raw, 8),
        death: DeathReason::from_u8(raw[12]),
        energy: u32_at(raw, 13),
        grid,
    })
}

// ---- RLE ----

/// `[count: u8][value: u8]` pairs, runs up to 255.
pub fn rle_compress(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let Some((&first, rest)) = data.split_first() else { return out };
    let (mut val, mut len) = (first, 1u8);
    for &b in rest {
        if b == val && len < 255 {
            len += 1;
        } else {
            out.extend_from_slice(&[len, val]);
            val = b;
            len = 1;
        }
    }
    out.extend_from_slice(&[len, val]);
    out
}

/// Decode to exactly `expected` bytes or `None` (odd length, zero-length run,
/// too long, too short). 0.1 decoded without a bound, so a stale tail after a
/// shorter rewrite produced more than 4096 bytes and a blank fossil.
pub fn rle_decompress(data: &[u8], expected: usize) -> Option<Vec<u8>> {
    if data.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(expected);
    for pair in data.chunks_exact(2) {
        let (n, v) = (pair[0] as usize, pair[1]);
        if n == 0 || out.len() + n > expected {
            return None;
        }
        out.resize(out.len() + n, v);
    }
    (out.len() == expected).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_grid() -> Grid {
        let mut g = Grid::new();
        for (i, c) in g.cells.iter_mut().enumerate() {
            *c = ((i / 7) % 7) as u8;
        }
        g
    }

    #[test]
    fn rle_round_trip_including_long_runs() {
        for data in [vec![0u8; 4096], vec![1, 1, 2, 3, 3, 3], vec![5], sample_grid().cells.to_vec()] {
            let c = rle_compress(&data);
            assert_eq!(rle_decompress(&c, data.len()).unwrap(), data);
        }
        assert_eq!(rle_compress(&[0u8; 300]), vec![255, 0, 45, 0], "runs split at 255");
        assert!(rle_compress(&[]).is_empty());
    }

    #[test]
    fn rle_rejects_bad_input() {
        assert!(rle_decompress(&[3, 1, 9], 3).is_none(), "odd length");
        assert!(rle_decompress(&[0, 1], 0).is_none(), "zero run");
        assert!(rle_decompress(&[4, 1], 3).is_none(), "too long");
        assert!(rle_decompress(&[2, 1], 3).is_none(), "too short");
    }

    #[test]
    fn touch_round_trip_and_legacy() {
        let mut v = vec![0u32; CELL_COUNT];
        v[0] = 5;
        v[4095] = 70_000;
        let t = TouchMap::from_vec(v.clone()).unwrap();
        let bytes = encode_touch(&t);
        assert_eq!(decode_touch(&bytes).unwrap(), t);
        let legacy: Vec<u8> = v.iter().flat_map(|c| c.to_le_bytes()).collect();
        assert_eq!(decode_touch(&legacy).unwrap(), t);
        assert_eq!(decode_touch(&bytes[..bytes.len() - 1]), Err(DecodeError::Length));
        assert_eq!(decode_touch(&[1, 2, 3]), Err(DecodeError::Format));
    }

    #[test]
    fn epoch_round_trip_and_legacy() {
        let g = sample_grid();
        let bytes = encode_epoch(3, 150, 0xBEEF, &g);
        let e = decode_epoch(&bytes).unwrap();
        assert_eq!((e.epoch, e.tick, e.seed), (3, 150, 0xBEEF));
        assert!(e.grid == g);
        assert!(decode_epoch(&bytes[HDR..]).is_ok(), "0.1 layout is the body without header");
    }

    /// Audit: a cell byte above 6 under the cursor panicked in draw_status at every launch.
    #[test]
    fn epoch_with_invalid_cell_is_rejected() {
        let mut bytes = encode_epoch(0, 0, 1, &Grid::new());
        bytes[HDR + 12 + 32 * 64 + 32] = 7;
        assert_eq!(decode_epoch(&bytes), Err(DecodeError::Value));
        let mut legacy = vec![0u8; 12 + CELL_COUNT];
        legacy[100] = 200;
        assert_eq!(decode_epoch(&legacy), Err(DecodeError::Value));
    }

    #[test]
    fn newer_versions_are_reported_not_misread() {
        let mut bytes = encode_epoch(0, 0, 1, &Grid::new());
        bytes[4] = VERSION + 1;
        assert_eq!(decode_epoch(&bytes), Err(DecodeError::NewerVersion(VERSION + 1)));
        let mut m = encode_meta(&Meta { last_wall_ms: Some(5) });
        m[4] = 9;
        assert_eq!(decode_meta(&m), Err(DecodeError::NewerVersion(9)));
    }

    #[test]
    fn meta_round_trip_legacy_and_unset() {
        let m = Meta { last_wall_ms: Some(1_700_000_000_000) };
        assert_eq!(decode_meta(&encode_meta(&m)).unwrap(), m);
        assert_eq!(decode_meta(&encode_meta(&Meta { last_wall_ms: None })).unwrap().last_wall_ms, None);
        let mut legacy = vec![0u8; 12];
        legacy[4] = 9;
        assert_eq!(decode_meta(&legacy).unwrap().last_wall_ms, None);
        assert_eq!(decode_meta(&[0; 3]), Err(DecodeError::Format));
    }

    fn fossil(epoch: u32, g: Grid) -> Fossil {
        Fossil { epoch, tick_count: 77, seed: 9, death: Some(DeathReason::Stagnation), energy: g.total_energy(), grid: g }
    }

    #[test]
    fn fossil_round_trip() {
        let f = fossil(12, sample_grid());
        assert_eq!(decode_fossil(&encode_fossil(&f)).unwrap(), f);
        let mut none = fossil(1, Grid::new());
        none.death = None;
        assert_eq!(decode_fossil(&encode_fossil(&none)).unwrap().death, None);
    }

    /// Audit: after epoch 100 a shorter RLE over a longer one left stale pairs and the fossil was blank.
    #[test]
    fn fossil_with_stale_tail_is_rejected_not_blank() {
        let long = encode_fossil(&fossil(1, sample_grid()));
        let short = encode_fossil(&fossil(101, Grid::new()));
        assert!(short.len() < long.len());
        // PDDB keeps the old tail after a shorter write
        let mut stale = short.clone();
        stale.extend_from_slice(&long[short.len()..]);
        assert_eq!(decode_fossil(&stale), Err(DecodeError::Length));
        assert!(decode_fossil(&short).is_ok());
    }

    #[test]
    fn fossil_legacy_and_corruption() {
        let f = fossil(5, sample_grid());
        let new = encode_fossil(&f);
        // build a 0.1 record: 17-byte header + rle
        let mut legacy = new[HDR..HDR + 17].to_vec();
        legacy.extend_from_slice(&new[HDR + 21..]);
        assert_eq!(decode_fossil(&legacy).unwrap(), f);
        assert!(decode_fossil(&new[..new.len() - 2]).is_err());
        let mut bad = new.clone();
        let last = bad.len() - 1;
        bad[last] = 99; // cell state 99
        assert_eq!(decode_fossil(&bad), Err(DecodeError::Value));
        assert!(decode_fossil(&[]).is_err());
    }

    #[test]
    fn fossil_slots_recycle_after_one_hundred() {
        assert_eq!(fossil_key(3), fossil_key(103));
        assert_ne!(fossil_key(3), fossil_key(4));
    }
}
