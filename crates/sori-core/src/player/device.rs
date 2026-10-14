//! 실제 오디오 장치(cpal). 윈도우는 WASAPI, 안드로이드는 AAudio, 리눅스는 ALSA.
//!
//! 장치가 받는 형식 가운데 정밀한 것(f64 > f32 > i32 > i24 > i16)을 고른다. 정수 형식이면
//! 마지막 단계에서 TPDF 디더를 넣는다. 16/24비트 원본을 손대지 않고 틀 때는 격자 위의
//! 값이라 디더가 들어가지 않는다(비트퍼펙트).

use std::sync::Arc;
use std::sync::atomic::Ordering;

use anyhow::{Context, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use rtrb::Consumer;

use crate::dsp::quantize::Quantizer;
use crate::player::output::{Callback, OutSpec, Output, SampleKind, Shared};

pub struct DeviceOutput {
    /// 고른 장치 이름. None이면 시스템 기본 장치.
    pub name: Option<String>,
    stream: Option<cpal::Stream>,
}

impl DeviceOutput {
    pub fn new(name: Option<String>) -> Self {
        DeviceOutput { name, stream: None }
    }

    fn device(&self) -> anyhow::Result<cpal::Device> {
        let host = cpal::default_host();
        if let Some(want) = &self.name
            && let Ok(devs) = host.output_devices()
        {
            for d in devs {
                if d.to_string() == *want {
                    return Ok(d);
                }
            }
        }
        host.default_output_device().ok_or_else(|| anyhow!("출력 장치가 없습니다"))
    }
}

/// 이 기기의 출력 장치 이름들.
pub fn list_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.output_devices().map(|ds| ds.map(|d| d.to_string()).collect()).unwrap_or_default()
}

fn rank(f: SampleFormat) -> u8 {
    match f {
        SampleFormat::F64 => 6,
        SampleFormat::F32 => 5,
        SampleFormat::I32 => 4,
        SampleFormat::I24 => 3,
        SampleFormat::I16 => 2,
        _ => 0,
    }
}

fn api_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "WASAPI 공유 모드"
    } else if cfg!(target_os = "android") {
        "AAudio"
    } else if cfg!(target_os = "macos") {
        "CoreAudio"
    } else {
        "ALSA"
    }
}

impl Output for DeviceOutput {
    fn probe(&mut self) -> anyhow::Result<(Vec<u32>, usize)> {
        let dev = self.device()?;
        let def = dev.default_output_config().context("장치 형식을 알 수 없습니다")?;
        let mut rates: Vec<u32> = Vec::new();
        if let Ok(ranges) = dev.supported_output_configs() {
            for r in ranges {
                for rate in [44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000] {
                    if r.min_sample_rate() <= rate && rate <= r.max_sample_rate() && !rates.contains(&rate) {
                        rates.push(rate);
                    }
                }
            }
        }
        rates.sort_unstable();
        Ok((rates, def.channels() as usize))
    }

    fn open(&mut self, rate: Option<u32>, consumer: Consumer<f64>, shared: Arc<Shared>) -> anyhow::Result<OutSpec> {
        self.close();
        let dev = self.device()?;
        let def = dev.default_output_config().context("장치 형식을 알 수 없습니다")?;
        let channels = def.channels();
        // 같은 채널 수의 형식 가운데 원하는 샘플레이트를 받고 가장 정밀한 것.
        let want_rate = rate.unwrap_or(def.sample_rate());
        let mut best: Option<cpal::SupportedStreamConfig> = None;
        if let Ok(ranges) = dev.supported_output_configs() {
            for r in ranges {
                if r.channels() != channels || rank(r.sample_format()) == 0 {
                    continue;
                }
                if let Some(c) = r.try_with_sample_rate(want_rate)
                    && best.as_ref().is_none_or(|b| rank(c.sample_format()) > rank(b.sample_format()))
                {
                    best = Some(c);
                }
            }
        }
        let chosen = best.unwrap_or(def);
        let format = chosen.sample_format();
        let config: StreamConfig = chosen.config();
        let rate = config.sample_rate;
        let ch = config.channels as usize;
        if ch != shared.channels {
            return Err(anyhow!("장치 채널 수가 바뀌었습니다({} → {ch})", shared.channels));
        }
        let fail = shared.clone();
        let on_error = move |e: cpal::Error| {
            tracing::warn!("출력 장치 오류: {e}");
            fail.failed.store(true, Ordering::Relaxed);
        };
        let mut cb = Callback::new(consumer, shared.clone());
        let mut buf: Vec<f64> = Vec::with_capacity(16_384);
        let stream = match format {
            SampleFormat::F64 => dev.build_output_stream::<f64, _, _>(
                config,
                move |data: &mut [f64], _| {
                    cb.fill(data);
                },
                on_error,
                None,
            )?,
            SampleFormat::F32 => dev.build_output_stream::<f32, _, _>(
                config,
                move |data: &mut [f32], _| {
                    buf.resize(data.len(), 0.0);
                    cb.fill(&mut buf);
                    for (o, s) in data.iter_mut().zip(&buf) {
                        *o = *s as f32;
                    }
                },
                on_error,
                None,
            )?,
            SampleFormat::I32 => {
                let mut q = Quantizer::new(32, true);
                dev.build_output_stream::<i32, _, _>(
                    config,
                    move |data: &mut [i32], _| {
                        buf.resize(data.len(), 0.0);
                        cb.fill(&mut buf);
                        let mut i = 0;
                        q.each(&buf, |v| {
                            data[i] = v;
                            i += 1;
                        });
                    },
                    on_error,
                    None,
                )?
            }
            SampleFormat::I24 => {
                let mut q = Quantizer::new(24, true);
                dev.build_output_stream::<cpal::I24, _, _>(
                    config,
                    move |data: &mut [cpal::I24], _| {
                        buf.resize(data.len(), 0.0);
                        cb.fill(&mut buf);
                        let mut i = 0;
                        q.each(&buf, |v| {
                            data[i] = cpal::I24::new(v).unwrap_or(cpal::I24::new(0).unwrap());
                            i += 1;
                        });
                    },
                    on_error,
                    None,
                )?
            }
            SampleFormat::I16 => {
                let mut q = Quantizer::new(16, true);
                dev.build_output_stream::<i16, _, _>(
                    config,
                    move |data: &mut [i16], _| {
                        buf.resize(data.len(), 0.0);
                        cb.fill(&mut buf);
                        let mut i = 0;
                        q.each(&buf, |v| {
                            data[i] = v as i16;
                            i += 1;
                        });
                    },
                    on_error,
                    None,
                )?
            }
            other => return Err(anyhow!("이 장치 형식은 아직 못 씁니다: {other:?}")),
        };
        stream.play().context("재생을 시작할 수 없습니다")?;
        self.stream = Some(stream);
        let kind = match format {
            SampleFormat::F64 => SampleKind::F64,
            SampleFormat::F32 => SampleKind::F32,
            SampleFormat::I32 => SampleKind::I32,
            SampleFormat::I24 => SampleKind::I24,
            _ => SampleKind::I16,
        };
        Ok(OutSpec { rate, channels: ch, format: kind, device: dev.to_string(), api: api_name().into(), mixer: true })
    }

    fn close(&mut self) {
        self.stream = None;
    }
}
