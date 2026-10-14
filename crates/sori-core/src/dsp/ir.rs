//! 컨볼루션용 임펄스 응답 파일. 데이터 폴더의 `ir/`에 둔다.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::config::write_atomic;
use crate::decode;
use crate::dsp::request::valid_ir_id;
use crate::dsp::resample::{Filter, Quality, Resampler};
use crate::model::now_ms;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IrInfo {
    pub id: String,
    pub name: String,
    pub ext: String,
    pub channels: usize,
    pub sample_rate: u32,
    pub duration: f64,
    pub size: u64,
    pub created_at: i64,
}

pub const IR_MAX_BYTES: usize = 50 * 1024 * 1024;

/// 채널별 샘플
pub type IrData = Arc<Vec<Vec<f64>>>;

pub struct IrStore {
    dir: PathBuf,
    cache: Mutex<HashMap<(String, u32), IrData>>,
}

impl IrStore {
    pub fn new(data_dir: &Path) -> Self {
        IrStore { dir: data_dir.join("ir"), cache: Mutex::new(HashMap::new()) }
    }

    fn index_file(&self) -> PathBuf {
        self.dir.join("index.json")
    }

    fn read_index(&self) -> Vec<IrInfo> {
        std::fs::read(self.index_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    fn write_index(&self, list: &[IrInfo]) -> std::io::Result<()> {
        write_atomic(&self.index_file(), &serde_json::to_vec_pretty(list).unwrap_or_default())
    }

    pub fn list(&self) -> Vec<IrInfo> {
        let mut v = self.read_index();
        v.sort_by_key(|x| std::cmp::Reverse(x.created_at));
        v
    }

    pub fn get(&self, id: &str) -> Option<(IrInfo, PathBuf)> {
        if !valid_ir_id(id) {
            return None;
        }
        let info = self.read_index().into_iter().find(|x| x.id == id)?;
        let file = self.dir.join(format!("{}.{}", info.id, info.ext));
        file.exists().then_some((info, file))
    }

    pub fn save(&self, name: &str, data: &[u8]) -> anyhow::Result<IrInfo> {
        let ext = Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !["wav", "flac", "aif", "aiff"].contains(&ext.as_str()) {
            anyhow::bail!("WAV, FLAC, AIFF 파일만 받습니다");
        }
        if data.len() > IR_MAX_BYTES {
            anyhow::bail!("50MB보다 큰 파일은 받지 않습니다");
        }
        let id: String = (0..18).map(|_| char::from_digit(rand::random_range(0..16u32), 16).unwrap()).collect();
        std::fs::create_dir_all(&self.dir)?;
        let file = self.dir.join(format!("{id}.{ext}"));
        std::fs::write(&file, data)?;
        let check = (|| -> anyhow::Result<(usize, u32, f64)> {
            let src = decode::open(&file).map_err(|_| anyhow::anyhow!("오디오 파일로 읽을 수 없습니다"))?;
            let s = src.spec().clone();
            Ok((s.channels, s.rate, s.duration().unwrap_or(0.0)))
        })();
        let (channels, rate, duration) = match check {
            Ok(v) => v,
            Err(e) => {
                let _ = std::fs::remove_file(&file);
                return Err(e);
            }
        };
        let bad = if !(1..=2).contains(&channels) {
            Some("모노나 스테레오 임펄스 응답만 쓸 수 있습니다")
        } else if duration > 30.0 {
            Some("30초보다 긴 임펄스 응답은 쓸 수 없습니다")
        } else {
            None
        };
        if let Some(msg) = bad {
            let _ = std::fs::remove_file(&file);
            anyhow::bail!(msg);
        }
        let base = Path::new(name).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let info = IrInfo {
            id,
            name: base.chars().take(120).collect(),
            ext,
            channels,
            sample_rate: rate,
            duration: (duration * 1000.0).round() / 1000.0,
            size: data.len() as u64,
            created_at: now_ms(),
        };
        let mut list = self.read_index();
        list.push(info.clone());
        self.write_index(&list)?;
        Ok(info)
    }

    pub fn delete(&self, id: &str) -> bool {
        let Some((_, file)) = self.get(id) else { return false };
        let _ = std::fs::remove_file(file);
        let list: Vec<IrInfo> = self.read_index().into_iter().filter(|x| x.id != id).collect();
        self.cache.lock().retain(|(k, _), _| k != id);
        self.write_index(&list).is_ok()
    }

    /// 채널별 샘플을 `rate`로 바꿔서. 한 번 읽은 것은 기억해 둔다.
    pub fn load(&self, id: &str, rate: u32) -> anyhow::Result<IrData> {
        if let Some(v) = self.cache.lock().get(&(id.to_string(), rate)) {
            return Ok(v.clone());
        }
        let (_, file) = self.get(id).ok_or_else(|| anyhow::anyhow!("임펄스 응답을 찾을 수 없습니다"))?;
        let (spec, mut data) = decode::decode_all(&file)?;
        if spec.rate != rate {
            let mut rs = Resampler::new(spec.rate, rate, spec.channels, Quality { bits: 28, filter: Filter::Precise });
            let mut out = Vec::new();
            rs.process(&data, &mut out);
            rs.flush(&mut out);
            data = out;
        }
        let ch = spec.channels.max(1);
        let chans: Vec<Vec<f64>> = (0..ch).map(|c| data.iter().skip(c).step_by(ch).copied().collect()).collect();
        let v = Arc::new(chans);
        let mut cache = self.cache.lock();
        if cache.len() > 16 {
            cache.clear();
        }
        cache.insert((id.to_string(), rate), v.clone());
        Ok(v)
    }
}
