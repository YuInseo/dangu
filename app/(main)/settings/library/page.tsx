import type { Metadata } from 'next';
import { AnalysisCard, RescanCard } from '@/components/settings-client';
import { config } from '@/lib/config';
import { getAnalysisStatus, getScanStatus, loadLibrary } from '@/lib/library/store';
import { bytes, count, span } from '@/lib/time';

export const metadata: Metadata = { title: '설정 · 라이브러리' };

export default async function LibrarySettings() {
  const lib = await loadLibrary();
  const s = lib.summary;
  return (
    <section>
      <h2 className="settings-h2">라이브러리</h2>
      <p className="settings-lede">음악 폴더는 읽기만 합니다. 색인과 표지, 분석 결과는 캐시 폴더에 두며 지워도 다시 만들어집니다.</p>
      <div className="pref-row">
        <div>
          <div className="pref-label">음악 폴더</div>
          <div className="pref-hint">
            서버를 띄울 때 <code>MUSIC_DIR</code>로 바꿉니다.
          </div>
        </div>
        <code className="pref-code">{config.musicDir}</code>
      </div>
      <div className="pref-row">
        <div className="pref-label">캐시 폴더</div>
        <code className="pref-code">{config.dataDir}</code>
      </div>
      <div className="pref-row">
        <div className="pref-label">들어 있는 것</div>
        <div className="pref-value">
          앨범 {count(s.albums)} · 아티스트 {count(s.artists)} · 곡 {count(s.tracks)} · {span(s.duration)} · {bytes(s.bytes)}
          <br />
          <span className="muted">
            무손실 앨범 {count(s.lossless)}장 · 그중 Hi-Res {count(s.hires)}장
          </span>
        </div>
      </div>
      <RescanCard initial={getScanStatus()} scannedAt={s.scannedAt} />
      <h3 className="pref-group">오디오 분석</h3>
      <AnalysisCard initial={getAnalysisStatus()} />
    </section>
  );
}
