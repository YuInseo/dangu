import type { Metadata } from 'next';
import { PageHeader, Section } from '@/components/section';
import { FormatSupport, OutputInfo, RescanCard } from '@/components/settings-client';
import { config, hasFfmpeg } from '@/lib/config';
import { getScanStatus, loadLibrary } from '@/lib/library/store';
import { bytes, count, span } from '@/lib/time';

export const metadata: Metadata = { title: '설정' };

export default async function SettingsPage() {
  const lib = await loadLibrary();
  const ffmpeg = await hasFfmpeg();
  const s = lib.summary;
  return (
    <>
      <PageHeader title="설정" />

      <Section title="라이브러리">
        <div className="panel">
          <dl className="kv">
            <dt>음악 폴더</dt>
            <dd>
              <code>{config.musicDir}</code>
            </dd>
            <dt>색인·표지 캐시</dt>
            <dd>
              <code>{config.dataDir}</code>
            </dd>
            <dt>들어 있는 것</dt>
            <dd>
              앨범 {count(s.albums)} · 아티스트 {count(s.artists)} · 곡 {count(s.tracks)} · {span(s.duration)} · {bytes(s.bytes)}
            </dd>
            <dt>무손실</dt>
            <dd>
              앨범 {count(s.lossless)}장 중 Hi-Res {count(s.hires)}장
            </dd>
          </dl>
          <RescanCard initial={getScanStatus()} scannedAt={s.scannedAt} />
        </div>
      </Section>

      <Section title="재생">
        <div className="panel">
          <dl className="kv">
            <dt>전송 방식</dt>
            <dd>원본 파일을 바이트 그대로 보냅니다. 서버에서 디코딩하거나 다시 압축하지 않습니다.</dd>
            <dt>ffmpeg</dt>
            <dd>
              {ffmpeg ? (
                <span className="ok">설치됨 — 브라우저가 못 여는 형식(ALAC, DSD, WavPack, APE 등)을 FLAC으로 바꿔 재생합니다.</span>
              ) : (
                <span className="warn">없음 — 브라우저가 직접 여는 형식만 재생됩니다. 설치하면 ALAC·DSD 등도 재생됩니다.</span>
              )}
            </dd>
          </dl>
          <OutputInfo />
        </div>
      </Section>

      <Section title="이 브라우저가 여는 형식">
        <FormatSupport ffmpeg={ffmpeg} />
      </Section>

      <Section title="정보">
        <div className="panel">
          <dl className="kv">
            <dt>버전</dt>
            <dd>Sori {config.version}</dd>
            <dt>단축키</dt>
            <dd className="keys">
              <span>
                <kbd>Space</kbd> 재생/일시정지
              </span>
              <span>
                <kbd>←</kbd>
                <kbd>→</kbd> 10초
              </span>
              <span>
                <kbd>Shift</kbd>+<kbd>←</kbd>
                <kbd>→</kbd> 이전/다음 곡
              </span>
              <span>
                <kbd>M</kbd> 음소거
              </span>
              <span>
                <kbd>/</kbd> 검색
              </span>
              <span>
                <kbd>Q</kbd> 대기열
              </span>
              <span>
                <kbd>F</kbd> 지금 재생 중
              </span>
            </dd>
            {config.password && (
              <>
                <dt>로그인</dt>
                <dd>
                  <form action="/api/logout" method="post">
                    <button type="submit" className="btn btn-sm">
                      이 기기에서 로그아웃
                    </button>
                  </form>
                </dd>
              </>
            )}
          </dl>
        </div>
      </Section>
    </>
  );
}
