import type { Metadata } from 'next';
import { FormatSupport, ZoneCard } from '@/components/settings-client';
import { getCaps } from '@/lib/dsp/caps';

export const metadata: Metadata = { title: '설정 · 오디오' };

function Cap({ on, label, hint }: { on: boolean; label: string; hint: string }) {
  return (
    <div className="pref-row">
      <div>
        <div className="pref-label">{label}</div>
        <div className="pref-hint">{hint}</div>
      </div>
      <span className={on ? 'ok' : 'warn'}>{on ? '쓸 수 있음' : '없음'}</span>
    </div>
  );
}

export default async function AudioSettings() {
  const caps = await getCaps();
  return (
    <section>
      <h2 className="settings-h2">오디오</h2>
      <p className="settings-lede">
        Sori에서는 곡을 트는 브라우저 하나하나가 존입니다. DSP 설정은 존마다 따로 저장됩니다. 브라우저는 출력 장치를 독점할 수 없어 늘
        운영체제 믹서를 지나므로, 비트퍼펙트로 들으려면 운영체제의 출력 샘플레이트를 곡에 맞추거나 DSP의 “출력 장치에 맞춤”을 켜세요.
      </p>
      <ZoneCard />
      <h3 className="pref-group">서버의 처리 능력</h3>
      <Cap
        on={caps.ffmpeg}
        label={`ffmpeg${caps.version ? ` ${caps.version}` : ''}`}
        hint="변환, DSP, 오디오 분석 모두 ffmpeg로 합니다. 없으면 원본 재생만 됩니다."
      />
      <Cap on={caps.soxr} label="SoX 리샘플러 (libsoxr)" hint="샘플레이트 변환. 없으면 ffmpeg 내장 리샘플러로 대신합니다." />
      <Cap on={caps.bs2b} label="Bauer 크로스피드 (libbs2b)" hint="크로스피드. 없으면 ffmpeg 내장 crossfeed로 비슷하게 합니다." />
      <Cap on={caps.afir} label="컨볼루션 (afir)" hint="임펄스 응답 컨볼루션." />
      <Cap on={caps.ebur128} label="EBU R128 측정 (ebur128)" hint="볼륨 레벨링용 라우드니스 분석." />
      <h3 className="pref-group">이 브라우저가 여는 형식</h3>
      <FormatSupport ffmpeg={caps.ffmpeg} />
    </section>
  );
}
