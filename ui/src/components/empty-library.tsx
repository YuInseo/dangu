import Link from '@/compat/link';
import type { ScanStatus } from '@/lib/library/types';
import { FolderOpen, Radio } from './icons';
import { LiveScanProgress } from './live-scan-progress';

export function EmptyLibrary({ status, musicDir }: { status: ScanStatus; musicDir: string }) {
  return (
    <div className="empty">
      <div className="empty-icon">
        <FolderOpen size={30} />
      </div>
      {status.state === 'scanning' ? (
        <>
          <h1>라이브러리를 읽고 있어요</h1>
          <p>처음 한 번은 태그와 표지를 전부 읽느라 시간이 걸립니다. 다음부터는 바뀐 파일만 봅니다.</p>
          <LiveScanProgress initial={status} />
        </>
      ) : (
        <>
          <h1>{status.state === 'error' ? '음악 폴더를 찾지 못했어요' : '음악 폴더가 비어 있어요'}</h1>
          <p>
            Sori는 아래 폴더의 음악을 원본 그대로 스트리밍합니다. FLAC, ALAC, WAV, AIFF, DSD, MP3, AAC를 넣거나, 서버를 띄울 때{' '}
            <code>MUSIC_DIR</code>로 다른 폴더를 가리키세요.
          </p>
          <code className="empty-path">{musicDir}</code>
          {status.state === 'error' && <p className="empty-error">{status.error}</p>}
        </>
      )}
      <div className="empty-alt">
        <Radio size={18} />
        <span>
          지금 바로 들을 수 있는 것도 있어요 — <Link href="/live">라이브 아카이브</Link>에는 아티스트가 공유를 허락한 공연 실황이 무손실
          FLAC으로 있습니다.
        </span>
      </div>
    </div>
  );
}
