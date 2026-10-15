//! 실제 오디오 장치(cpal). 윈도우는 WASAPI, 안드로이드는 AAudio, 리눅스는 ALSA.
//!
//! 형식은 32비트 실수(f32)를 먼저 고른다. 윈도우 공유 모드와 안드로이드의 믹서가 안에서 쓰는
//! 형식이라 바꾸는 단계가 없고, 16/24비트 원본은 f32 안에 그대로 들어가 비트퍼펙트다. f32가 없으면
//! 정수 형식(i32 > i24 > i16)을 쓰고 마지막 단계에서 TPDF 디더를 넣는다.
//!
//! 장치가 "된다"고 알려 준 형식도 실제로는 열리지 않을 수 있다(윈도우가 f64를 거부하는 등). 그래서
//! 후보를 하나씩 실제로 열어 보고, 다 안 되면 장치의 기본 형식으로 연다.

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
        SampleFormat::F32 => 6,
        SampleFormat::I32 => 4,
        SampleFormat::I24 => 3,
        SampleFormat::I16 => 2,
        // 믹서가 f64를 받는 장치는 드물고, 받는다고 해도 얻는 것이 없다.
        SampleFormat::F64 => 1,
        _ => 0,
    }
}

/// 이 설정으로 장치가 실제로 열리는지(스트림을 만들어 보고 바로 닫는다).
fn opens(dev: &cpal::Device, c: &cpal::SupportedStreamConfig) -> bool {
    let res = dev.build_output_stream_raw(
        c.config(),
        c.sample_format(),
        |data: &mut cpal::Data, _: &cpal::OutputCallbackInfo| data.bytes_mut().fill(0),
        |_| {},
        None,
    );
    match res {
        Ok(_) => true,
        Err(e) => {
            tracing::info!(format = ?c.sample_format(), rate = c.sample_rate(), "장치가 이 형식을 받지 않습니다: {e}");
            false
        }
    }
}

fn same(a: &cpal::SupportedStreamConfig, b: &cpal::SupportedStreamConfig) -> bool {
    a.sample_format() == b.sample_format() && a.sample_rate() == b.sample_rate() && a.channels() == b.channels()
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
        // 같은 채널 수에서 원하는 샘플레이트를 받는 형식들(좋은 것부터), 그리고 장치 기본 형식.
        let want_rate = rate.unwrap_or(def.sample_rate());
        let mut candidates: Vec<cpal::SupportedStreamConfig> = Vec::new();
        if let Ok(ranges) = dev.supported_output_configs() {
            for r in ranges {
                if r.channels() != channels || rank(r.sample_format()) == 0 {
                    continue;
                }
                if let Some(c) = r.try_with_sample_rate(want_rate)
                    && !candidates.iter().any(|x| same(x, &c))
                {
                    candidates.push(c);
                }
            }
        }
        candidates.sort_by_key(|c| std::cmp::Reverse(rank(c.sample_format())));
        // 기본 형식보다 못한 것은 볼 필요가 없다.
        candidates.retain(|c| rank(c.sample_format()) > rank(def.sample_format()) || same(c, &def));
        let chosen = candidates.into_iter().find(|c| same(c, &def) || opens(&dev, c)).unwrap_or(def);
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
