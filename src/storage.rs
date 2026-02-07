use std::io::{Read, Write, Seek, SeekFrom};
use crate::grid::{Grid, CELL_COUNT};
use crate::rules::DeathReason;

const DICT: &str = "carse.data";
const KEY_TOUCHMAP: &str = "touchmap";
const KEY_EPOCH_CURRENT: &str = "epoch.current";
const KEY_EPOCH_META: &str = "epoch.meta";
const MAX_FOSSILS: u32 = 100;

pub struct CarseStorage {
    pddb: pddb::Pddb,
}

// For M.D.C. — the infinite game that matters most.

pub struct FossilRecord {
    pub epoch: u32,
    pub tick_count: u32,
    pub seed: u32,
    pub death_reason: Option<DeathReason>,
    pub energy: u32,
    pub grid: Grid,
}

pub struct EpochMeta {
    pub epoch_counter: u32,
    pub last_session_ms: u64,
}

impl CarseStorage {
    pub fn new() -> Self {
        let pddb = pddb::Pddb::new();
        pddb.try_mount();
        Self { pddb }
    }

    // --- Touch Map ---

    pub fn save_touch_map(&self, touch_map: &[u32; CELL_COUNT]) {
        match self.pddb.get(DICT, KEY_TOUCHMAP, None, true, true, Some(CELL_COUNT * 4), None::<fn()>) {
            Ok(mut key) => {
                key.seek(SeekFrom::Start(0)).ok();
                for &val in touch_map.iter() {
                    key.write_all(&val.to_le_bytes()).ok();
                }
                self.pddb.sync().ok();
            }
            Err(e) => log::warn!("save touch map: {:?}", e),
        }
    }

    pub fn load_touch_map(&self) -> Option<[u32; CELL_COUNT]> {
        match self.pddb.get(DICT, KEY_TOUCHMAP, None, false, false, None, None::<fn()>) {
            Ok(mut key) => {
                let mut result = [0u32; CELL_COUNT];
                key.seek(SeekFrom::Start(0)).ok();
                for val in result.iter_mut() {
                    let mut buf = [0u8; 4];
                    if key.read_exact(&mut buf).is_err() { return None; }
                    *val = u32::from_le_bytes(buf);
                }
                Some(result)
            }
            Err(_) => None,
        }
    }

    // --- Epoch State ---

    pub fn save_epoch_state(&self, epoch: u32, tick: u32, seed: u32, grid: &Grid) {
        let size = 12 + CELL_COUNT;
        match self.pddb.get(DICT, KEY_EPOCH_CURRENT, None, true, true, Some(size), None::<fn()>) {
            Ok(mut key) => {
                key.seek(SeekFrom::Start(0)).ok();
                key.write_all(&epoch.to_le_bytes()).ok();
                key.write_all(&tick.to_le_bytes()).ok();
                key.write_all(&seed.to_le_bytes()).ok();
                key.write_all(&grid.cells).ok();
                self.pddb.sync().ok();
            }
            Err(e) => log::warn!("save epoch state: {:?}", e),
        }
    }

    pub fn load_epoch_state(&self) -> Option<(u32, u32, u32, Grid)> {
        match self.pddb.get(DICT, KEY_EPOCH_CURRENT, None, false, false, None, None::<fn()>) {
            Ok(mut key) => {
                key.seek(SeekFrom::Start(0)).ok();
                let mut header = [0u8; 12];
                if key.read_exact(&mut header).is_err() { return None; }
                let epoch = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
                let tick = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
                let seed = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
                let mut grid = Grid::new();
                if key.read_exact(&mut grid.cells).is_err() { return None; }
                Some((epoch, tick, seed, grid))
            }
            Err(_) => None,
        }
    }

    // --- Epoch Meta ---

    pub fn save_epoch_meta(&self, meta: &EpochMeta) {
        match self.pddb.get(DICT, KEY_EPOCH_META, None, true, true, Some(12), None::<fn()>) {
            Ok(mut key) => {
                key.seek(SeekFrom::Start(0)).ok();
                key.write_all(&meta.epoch_counter.to_le_bytes()).ok();
                key.write_all(&meta.last_session_ms.to_le_bytes()).ok();
                self.pddb.sync().ok();
            }
            Err(e) => log::warn!("save epoch meta: {:?}", e),
        }
    }

    pub fn load_epoch_meta(&self) -> Option<EpochMeta> {
        match self.pddb.get(DICT, KEY_EPOCH_META, None, false, false, None, None::<fn()>) {
            Ok(mut key) => {
                key.seek(SeekFrom::Start(0)).ok();
                let mut buf = [0u8; 12];
                if key.read_exact(&mut buf).is_err() { return None; }
                Some(EpochMeta {
                    epoch_counter: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
                    last_session_ms: u64::from_le_bytes([
                        buf[4], buf[5], buf[6], buf[7],
                        buf[8], buf[9], buf[10], buf[11],
                    ]),
                })
            }
            Err(_) => None,
        }
    }

    // --- Fossil Record ---

    pub fn save_fossil(&self, epoch: u32, tick_count: u32, seed: u32,
                       death_reason: DeathReason, energy: u32, grid: &Grid) {
        let key_name = format!("fossil.{}", epoch % MAX_FOSSILS);
        let compressed = rle_compress(&grid.cells);
        let size = 17 + compressed.len();

        match self.pddb.get(DICT, &key_name, None, true, true, Some(size), None::<fn()>) {
            Ok(mut key) => {
                key.seek(SeekFrom::Start(0)).ok();
                key.write_all(&epoch.to_le_bytes()).ok();
                key.write_all(&tick_count.to_le_bytes()).ok();
                key.write_all(&seed.to_le_bytes()).ok();
                key.write_all(&[death_reason.as_u8()]).ok();
                key.write_all(&energy.to_le_bytes()).ok();
                key.write_all(&compressed).ok();
                self.pddb.sync().ok();
            }
            Err(e) => log::warn!("save fossil: {:?}", e),
        }
    }

    pub fn load_fossil(&self, epoch: u32) -> Option<FossilRecord> {
        let key_name = format!("fossil.{}", epoch % MAX_FOSSILS);
        match self.pddb.get(DICT, &key_name, None, false, false, None, None::<fn()>) {
            Ok(mut key) => {
                key.seek(SeekFrom::Start(0)).ok();
                let mut header = [0u8; 17];
                if key.read_exact(&mut header).is_err() { return None; }
                let stored_epoch = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
                if stored_epoch != epoch { return None; } // Slot was recycled
                let tick_count = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
                let seed = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
                let death_reason = DeathReason::from_u8(header[12]);
                let energy = u32::from_le_bytes([header[13], header[14], header[15], header[16]]);

                let mut compressed = Vec::new();
                key.read_to_end(&mut compressed).ok();
                let decompressed = rle_decompress(&compressed);
                let mut grid = Grid::new();
                if decompressed.len() == CELL_COUNT {
                    grid.cells.copy_from_slice(&decompressed);
                }
                Some(FossilRecord { epoch: stored_epoch, tick_count, seed, death_reason, energy, grid })
            }
            Err(_) => None,
        }
    }
}

/// Simple RLE compression: [count:u8][value:u8] pairs.
fn rle_compress(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    if data.is_empty() { return result; }

    let mut run_val = data[0];
    let mut run_len: u8 = 1;

    for &byte in &data[1..] {
        if byte == run_val && run_len < 255 {
            run_len += 1;
        } else {
            result.push(run_len);
            result.push(run_val);
            run_val = byte;
            run_len = 1;
        }
    }
    result.push(run_len);
    result.push(run_val);
    result
}

fn rle_decompress(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut i = 0;
    while i + 1 < data.len() {
        let count = data[i] as usize;
        let val = data[i + 1];
        for _ in 0..count {
            result.push(val);
        }
        i += 2;
    }
    result
}
